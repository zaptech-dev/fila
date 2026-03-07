mod m20260307_131338_create_merge_events;

mod m20260307_130846_create_batchs;

mod m20260307_033928_create_pull_requests;

rapina::migrations! {
    m20260307_033928_create_pull_requests,
    m20260307_130846_create_batchs,
    m20260307_131338_create_merge_events,
}
