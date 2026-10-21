use crate::api::wire::DeviceView;
use crate::contracts::{
    DEVICE_AGE_RULE_MS, Device, DeviceActions, DeviceStatus, HistoryGate, Millis, Tier,
};

pub fn current(views: &[DeviceView]) -> Option<&DeviceView> {
    views.iter().find(|view| view.current)
}

pub fn gate(current: &DeviceView, now: Millis) -> HistoryGate {
    let unlocks_at = current.created_at + DEVICE_AGE_RULE_MS;
    let locked = !current.history_unlocked && now < unlocks_at;
    HistoryGate {
        locked,
        unlocks_at: locked.then_some(unlocks_at),
    }
}

pub fn to_devices(views: &[DeviceView], now: Millis) -> Vec<Device> {
    let me = current(views);
    views
        .iter()
        .map(|view| Device {
            id: view.id,
            platform: view.platform.clone(),
            name: view.name.clone(),
            app_version: view.app_version.clone(),
            tier: tier(&view.tier),
            status: view.status,
            created_at: view.created_at,
            last_seen_at: view.last_seen_at,
            history_unlocked: view.history_unlocked,
            current: view.current,
            actions: actions(me, view, now),
        })
        .collect()
}

pub fn awaiting_alert(views: &[DeviceView], now: Millis) -> Vec<&DeviceView> {
    let Some(me) = current(views) else {
        return Vec::new();
    };
    views
        .iter()
        .filter(|view| {
            !view.current
                && view.status == DeviceStatus::Active
                && !view.history_unlocked
                && view.created_at > me.created_at
                && now - view.created_at < DEVICE_AGE_RULE_MS
        })
        .collect()
}

fn actions(me: Option<&DeviceView>, view: &DeviceView, now: Millis) -> DeviceActions {
    let Some(me) = me else {
        return DeviceActions::default();
    };
    if view.current {
        return DeviceActions {
            remove: true,
            ..DeviceActions::default()
        };
    }
    let younger = view.created_at > me.created_at;
    let seasoned = now - me.created_at >= DEVICE_AGE_RULE_MS;
    let active = view.status == DeviceStatus::Active;
    DeviceActions {
        confirm: active && younger && seasoned && !view.history_unlocked,
        not_me: active && younger,
        remove: active && (younger || seasoned),
    }
}

fn tier(raw: &str) -> Tier {
    match raw {
        "relay" => Tier::Relay,
        "pro" => Tier::Pro,
        _ => Tier::Direct,
    }
}
