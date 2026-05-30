use crate::session::Scope;

pub fn path_snapshot(scope: Scope) -> String {
    format!("/federation/{}/snapshot", scope.as_str())
}

pub fn path_changes(scope: Scope) -> String {
    format!("/federation/{}/changes", scope.as_str())
}
