use std::collections::HashMap;

use super::local::Local;
use super::marks::MarkBatch;
use super::signal::{First, first};
use super::tiers::{BACKOFF_MAX_MS, BACKOFF_START_MS, MID_HOLD_MIN_MS};
use super::{PollPlan, SyncEngine, SyncOutcome, fatal, jitter};
use crate::api::SyncReply;
use crate::api::wire::{KNOWN_MAX, SyncRequest};
use crate::channel::BUSY_RETRY_MAX_MS;
use crate::contracts::{ChatError, ChatResult, Clock, ConvId, Millis, Seq};

pub const RESET_JITTER_MS: u64 = 10_000;
pub const RECONNECT_JITTER_MS: u64 = 10_000;
pub const LIMITED_RETRY_MS: u64 = 15_000;

struct Polled {
    started: Millis,
    marks: MarkBatch,
    known: HashMap<ConvId, Seq>,
    result: ChatResult<SyncReply>,
}

impl<L: Local> SyncEngine<L> {
    pub async fn run(&self, clock: &dyn Clock) -> ChatResult<()> {
        if let Err(error) = self.load_tally() {
            self.failed(error)?;
        }
        if let Err(error) = self.devices().await {
            self.failed(error)?;
        }
        while !self.stopped() {
            self.wake.reset();
            if self.reset_pending() {
                self.nap(clock, jitter::upto(RESET_JITTER_MS)).await;
                if self.stopped() {
                    break;
                }
                if let Err(error) = self.reconcile().await {
                    let after = self.failed(error)?;
                    self.pause(clock, after).await;
                    continue;
                }
            }
            if let Err(error) = self.tick_gate().await {
                self.failed(error)?;
            }
            if let Err(error) = self.flush_outbox().await {
                self.failed(error)?;
            }
            let plan = match self.next_plan() {
                Ok(plan) => plan,
                Err(error) => {
                    let after = self.failed(error)?;
                    self.pause(clock, after).await;
                    continue;
                }
            };
            let polled = match first(self.poll(plan), self.wake.wait()).await {
                First::Left(polled) => polled,
                First::Right(()) => continue,
            };
            let pause = match self.settle(plan, polled).await? {
                SyncOutcome::Changed | SyncOutcome::Idle if self.with(|state| state.more) => 0,
                SyncOutcome::Changed | SyncOutcome::Idle => plan.delay_ms,
                SyncOutcome::Reset => 0,
                SyncOutcome::Retry { after_ms } => after_ms,
            };
            self.pause(clock, pause).await;
        }
        Ok(())
    }

    pub async fn step(&self, plan: PollPlan) -> ChatResult<SyncOutcome> {
        let polled = self.poll(plan).await;
        self.settle(plan, polled).await
    }

    async fn pause(&self, clock: &dyn Clock, ms: u64) {
        if ms > 0 && !self.stopped() {
            first(clock.sleep(ms), self.wake.wait()).await;
        }
    }

    async fn nap(&self, clock: &dyn Clock, ms: u64) {
        if ms > 0 && !self.stopped() {
            first(clock.sleep(ms), self.halt.wait()).await;
        }
    }

    async fn poll(&self, plan: PollPlan) -> Polled {
        let started = self.now();
        let marks = self.with(|state| state.marks.batch());
        let (known, result) = match self.request(plan, &marks) {
            Ok((request, known)) => (known, self.api.sync(&request, plan.tier).await),
            Err(error) => (HashMap::new(), Err(error)),
        };
        Polled {
            started,
            marks,
            known,
            result,
        }
    }

    fn request(
        &self,
        plan: PollPlan,
        marks: &MarkBatch,
    ) -> ChatResult<(SyncRequest, HashMap<ConvId, Seq>)> {
        let sync = self.store.sync_state()?;
        let known: Vec<(ConvId, Seq)> = self
            .store
            .known(KNOWN_MAX)?
            .into_iter()
            .filter(|(_, seq)| *seq >= 0)
            .collect();
        let request = SyncRequest {
            pos: sync.pos,
            inc: sync.inc,
            hold_ms: plan.hold_ms,
            max_bytes: plan.tier.max_bytes(),
            known: known.clone(),
            focus: self.focus().filter(|_| self.visible()),
            read: marks.read.clone(),
            delivered: marks.delivered.clone(),
        };
        Ok((request, known.into_iter().collect()))
    }

    async fn settle(&self, plan: PollPlan, polled: Polled) -> ChatResult<SyncOutcome> {
        let elapsed = u64::try_from(self.now() - polled.started).unwrap_or_default();
        let reply = match polled.result {
            Ok(reply) => reply,
            Err(error) => return self.missed(plan, error, elapsed),
        };
        self.with(|state| {
            state.marks.commit(&polled.marks);
            state.tiers.answered();
            state.errors = 0;
        });
        self.api.note_tier(plan.tier);
        self.reached(plan.tier);
        match reply {
            SyncReply::Idle if plan.hold_ms > 0 && elapsed < MID_HOLD_MIN_MS => {
                Ok(SyncOutcome::Retry {
                    after_ms: MID_HOLD_MIN_MS,
                })
            }
            SyncReply::Idle => {
                self.with(|state| {
                    state.tiers.held();
                    state.more = false;
                });
                Ok(SyncOutcome::Idle)
            }
            SyncReply::Later { retry_ms } => Ok(SyncOutcome::Retry {
                after_ms: retry_ms.min(BUSY_RETRY_MAX_MS),
            }),
            SyncReply::Answer(answer) => match self.apply(*answer, &polled.known).await {
                Ok(outcome) => Ok(outcome),
                Err(error) => Ok(SyncOutcome::Retry {
                    after_ms: self.failed(error)?,
                }),
            },
        }
    }

    fn missed(&self, plan: PollPlan, error: ChatError, elapsed: u64) -> ChatResult<SyncOutcome> {
        if error != ChatError::Offline {
            return Ok(SyncOutcome::Retry {
                after_ms: self.failed(error)?,
            });
        }
        let order = self.api.transport().tiers();
        let now = self.now();
        let dropped = plan.hold_ms > 0 && elapsed >= MID_HOLD_MIN_MS && elapsed < plan.hold_ms;
        let after_ms = self.with(|state| {
            if dropped {
                state.tiers.dropped(plan.tier, &order, now)
            } else {
                state.tiers.unreachable(&order, now)
            }
        });
        let after_ms = match (dropped, after_ms) {
            (true, 0) => jitter::upto(RECONNECT_JITTER_MS),
            (_, after_ms) => after_ms,
        };
        if after_ms > 0 {
            self.lost();
        }
        Ok(SyncOutcome::Retry { after_ms })
    }

    fn failed(&self, error: ChatError) -> ChatResult<u64> {
        if fatal(&error) {
            self.lost();
            return Err(error);
        }
        Ok(match error {
            ChatError::Busy { retry_ms } if retry_ms > 0 => {
                let asked = retry_ms.min(BUSY_RETRY_MAX_MS);
                asked + jitter::upto(asked / 2)
            }
            ChatError::Busy { .. } | ChatError::Offline => jitter::upto(RECONNECT_JITTER_MS),
            ChatError::ReadLimited | ChatError::RequestLimit | ChatError::SendLimited => {
                LIMITED_RETRY_MS
            }
            _ => {
                let errors = self.with(|state| {
                    state.errors += 1;
                    state.errors
                });
                jitter::around(jitter::doubling(errors, BACKOFF_START_MS, BACKOFF_MAX_MS))
            }
        })
    }
}
