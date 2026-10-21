use uuid::Uuid;

uniffi::custom_type!(Uuid, String, {
    remote,
    lower: |id| id.to_string(),
    try_lift: |text| Ok(Uuid::parse_str(&text)?),
});
