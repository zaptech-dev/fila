use rapina::prelude::*;

schema! {
    #[timestamps(none)]
    PullRequest {
        repo_owner: String,
        repo_name: String,
        pr_number: i32,
        title: String,
        author: String,
        head_sha: String,
        status: String,
        priority: i32,
        installation_id: i64,
        queued_at: Option<DateTime>,
        merged_at: Option<DateTime>,
    }
}
schema! {
    #[timestamps(none)]
    Batch {
        status: String,
        completed_at: Option<DateTime>,
    }
}
schema! {
    #[timestamps(none)]
    MergeEvent {
        pull_request_id: i32,
        batch_id: i32,
        event_type: String,
        details: Option<String>,
    }
}
