use runner_core::protocol::{ClientResult, DaemonClient};

pub(super) fn resume_mission(
    daemon: &DaemonClient,
    mission: &str,
    size: (u16, u16),
) -> ClientResult<super::client::MissionResume> {
    use runner_core::protocol::model::SessionStatus;
    let mut sessions = daemon.session_list(mission)?;
    let mut first_error = None;
    for candidate in sessions.clone() {
        let already_running = sessions
            .iter()
            .find(|session| session.session.id == candidate.session.id)
            .is_some_and(|session| session.session.status == SessionStatus::Running);
        if already_running {
            continue;
        }
        match daemon.session_resume(&candidate.session.id, Some(size.0), Some(size.1)) {
            Ok(_) => {}
            Err(error) => {
                let message = error.to_string();
                if super::client::is_concurrent_resume_error(&message) {
                    match daemon.session_list(mission) {
                        Ok(refreshed) => sessions = refreshed,
                        Err(error) => {
                            first_error.get_or_insert_with(|| error.to_string());
                        }
                    }
                } else {
                    first_error.get_or_insert(message);
                }
            }
        }
    }
    match daemon.session_list(mission) {
        Ok(refreshed) => sessions = refreshed,
        Err(error) => {
            first_error.get_or_insert_with(|| error.to_string());
        }
    }
    Ok(super::client::MissionResume {
        local_sessions: sessions,
        error: first_error,
    })
}
