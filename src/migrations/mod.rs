mod m20260307_131338_create_merge_events;

mod m20260307_130846_create_batchs;

mod m20260307_033928_create_pull_requests;

mod m20260307_200000_add_installation_id;

mod m20260308_120000_add_approved_by;

mod m20260308_200000_add_indexes;

rapina::migrations! {
    m20260307_033928_create_pull_requests,
    m20260307_130846_create_batchs,
    m20260307_131338_create_merge_events,
    m20260307_200000_add_installation_id,
    m20260308_120000_add_approved_by,
    m20260308_200000_add_indexes,
}
