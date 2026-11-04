//! Build provenance determines telemetry routing independently of optimization.
pub const INPUTS: [&str; 6] = [
    "GLIM_DISTRIBUTION",
    "GITHUB_ACTIONS",
    "GITHUB_REPOSITORY",
    "GITHUB_EVENT_NAME",
    "GITHUB_REF",
    "PROFILE",
];

pub fn environment(value: impl Fn(&str) -> Option<String>) -> &'static str {
    let matches = |key, expected| value(key).as_deref() == Some(expected);
    if matches("GLIM_DISTRIBUTION", "github-release")
        && matches("GITHUB_ACTIONS", "true")
        && matches("GITHUB_REPOSITORY", "glimapp/glim")
        && matches("GITHUB_EVENT_NAME", "push")
        && value("GITHUB_REF").is_some_and(|reference| reference.starts_with("refs/tags/v"))
        && matches("PROFILE", "release")
    {
        "release"
    } else {
        "debug"
    }
}

#[cfg(test)]
mod tests {
    use super::{INPUTS, environment};
    use std::collections::HashMap;

    fn official() -> HashMap<&'static str, String> {
        HashMap::from([
            ("GLIM_DISTRIBUTION", "github-release".into()),
            ("GITHUB_ACTIONS", "true".into()),
            ("GITHUB_REPOSITORY", "glimapp/glim".into()),
            ("GITHUB_EVENT_NAME", "push".into()),
            ("GITHUB_REF", "refs/tags/v0.2.22".into()),
            ("PROFILE", "release".into()),
        ])
    }
    #[test]
    fn only_official_tag_release_builds_enter_production_telemetry() {
        let base = official();
        assert_eq!(environment(|key| base.get(key).cloned()), "release");
        for key in INPUTS {
            let mut values = base.clone();
            values.remove(key);
            assert_eq!(environment(|key| values.get(key).cloned()), "debug");
        }
        for (key, changed) in [
            ("PROFILE", "debug"),
            ("GITHUB_REF", "refs/heads/main"),
            ("GITHUB_EVENT_NAME", "workflow_dispatch"),
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_REPOSITORY", "contributor/glim"),
            ("GLIM_DISTRIBUTION", "development"),
        ] {
            let mut values = base.clone();
            values.insert(key, changed.into());
            assert_eq!(environment(|key| values.get(key).cloned()), "debug");
        }
    }
    #[test]
    fn local_optimized_builds_stay_debug_even_with_a_requested_release_marker() {
        for profile in ["debug", "release"] {
            let values = HashMap::from([
                ("PROFILE", profile.to_owned()),
                ("GLIM_DISTRIBUTION", "github-release".into()),
            ]);
            assert_eq!(environment(|key| values.get(key).cloned()), "debug");
        }
    }
}
