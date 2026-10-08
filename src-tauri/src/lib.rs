mod auth;
mod changes;
mod commit;
mod graph;
mod history;
mod repo;
mod sidebar;
mod terminal;
mod toolbar;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Git started us as its askpass helper (see auth): answer the prompt and exit before any UI exists.
    if std::env::args().nth(1).as_deref() == Some("--askpass") {
        std::process::exit(auth::askpass_main());
    }
    tauri::Builder::default()
        .manage(repo::watch::RepoWatcher::default())
        .manage(terminal::Terminal::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            auth::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth::answer_credentials,
            commit::file_history::get_file_history,
            graph::fast_forward::fast_forward_cmd,
            graph::merge::merge_branch_cmd,
            sidebar::branches::checkout_local_branch,
            sidebar::branches::checkout_remote_branch,
            sidebar::branches::count_unmerged_commits,
            sidebar::branches::create_branch,
            sidebar::branches::delete_local_branch,
            sidebar::branches::get_local_branches,
            sidebar::branches::rename_local_branch,
            changes::get_head_commit,
            changes::get_status,
            commit::get_commit_detail,
            commit::get_file_diff,
            commit::get_working_diff,
            changes::stage_paths,
            changes::unstage_paths,
            changes::discard_paths,
            changes::create_commit,
            graph::get_graph,
            history::drop::drop_latest_commit,
            history::drop::get_drop_info,
            history::rename::get_rename_info,
            changes::hunks::discard_hunk_cmd,
            changes::hunks::stage_hunk_cmd,
            changes::hunks::unstage_hunk_cmd,
            history::rename::rename_commit_message,
            history::rebase::get_rebase_plan,
            history::rebase::apply_rebase_cmd,
            repo::open_repo,
            repo::get_recent_repos,
            repo::remove_recent_repo,
            graph::reset::get_reset_info,
            graph::reset::reset_to_commit,
            sidebar::remotes::count_unmerged_remote_commits,
            sidebar::remotes::delete_remote_branch,
            sidebar::remotes::get_remotes,
            sidebar::remotes::rename_remote_branch,
            sidebar::remotes::add_remote_cmd,
            sidebar::remotes::set_remote_url_cmd,
            sidebar::remotes::delete_remote_cmd,
            sidebar::remotes::set_target_remote,
            sidebar::stash::create_stash,
            sidebar::stash::get_stashes,
            sidebar::stash::pop_stash_cmd,
            sidebar::stash::stash_paths_cmd,
            sidebar::stash::drop_stash_cmd,
            toolbar::sync::get_divergence,
            toolbar::sync::get_sync_status,
            toolbar::sync::git_fetch,
            toolbar::sync::git_auto_fetch,
            toolbar::sync::git_force_push,
            toolbar::sync::git_pull,
            toolbar::sync::git_pull_with,
            toolbar::sync::git_push,
            terminal::term_start,
            terminal::term_write,
            terminal::term_resize,
            terminal::term_stop,
            repo::watch::watch_repo,
            repo::watch::unwatch_repo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
