use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Public struct `GithubUser` used across Tau components.
pub struct GithubUser {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Public struct `GithubIssueLabel` used across Tau components.
pub struct GithubIssueLabel {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Public struct `GithubIssue` used across Tau components.
pub struct GithubIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub user: GithubUser,
    #[serde(default)]
    pub labels: Vec<GithubIssueLabel>,
    #[serde(default)]
    pub pull_request: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Public struct `GithubIssueComment` used across Tau components.
pub struct GithubIssueComment {
    pub id: u64,
    pub body: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub user: GithubUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Enumerates supported `GithubBridgeEventKind` values.
pub enum GithubBridgeEventKind {
    Opened,
    CommentCreated,
    CommentEdited,
}

impl GithubBridgeEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opened => "issue_opened",
            Self::CommentCreated => "issue_comment_created",
            Self::CommentEdited => "issue_comment_edited",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Public struct `GithubBridgeEvent` used across Tau components.
pub struct GithubBridgeEvent {
    pub key: String,
    pub kind: GithubBridgeEventKind,
    pub issue_number: u64,
    pub issue_title: String,
    pub author_login: String,
    pub occurred_at: String,
    pub body: String,
    pub raw_payload: Value,
}

pub fn collect_issue_events(
    issue: &GithubIssue,
    comments: &[GithubIssueComment],
    bot_login: &str,
    include_issue_body: bool,
    include_edited_comments: bool,
) -> Vec<GithubBridgeEvent> {
    let mut events = Vec::new();
    if include_issue_body
        && issue.user.login != bot_login
        && !issue.body.as_deref().unwrap_or_default().trim().is_empty()
    {
        let body = issue.body.clone().unwrap_or_default();
        events.push(GithubBridgeEvent {
            key: format!("issue-opened:{}", issue.id),
            kind: GithubBridgeEventKind::Opened,
            issue_number: issue.number,
            issue_title: issue.title.clone(),
            author_login: issue.user.login.clone(),
            occurred_at: issue.created_at.clone(),
            body,
            raw_payload: serde_json::to_value(issue).unwrap_or(Value::Null),
        });
    }

    for comment in comments {
        if comment.user.login == bot_login {
            continue;
        }
        let body = comment
            .body
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        if body.is_empty() {
            continue;
        }
        let is_edit = comment.updated_at != comment.created_at;
        if is_edit && !include_edited_comments {
            continue;
        }
        let (key, kind) = if is_edit {
            (
                format!("issue-comment-edited:{}:{}", comment.id, comment.updated_at),
                GithubBridgeEventKind::CommentEdited,
            )
        } else {
            (
                format!("issue-comment-created:{}", comment.id),
                GithubBridgeEventKind::CommentCreated,
            )
        };
        events.push(GithubBridgeEvent {
            key,
            kind,
            issue_number: issue.number,
            issue_title: issue.title.clone(),
            author_login: comment.user.login.clone(),
            occurred_at: comment.created_at.clone(),
            body: body.to_string(),
            raw_payload: serde_json::to_value(comment).unwrap_or(Value::Null),
        });
    }

    events.sort_by(|left, right| {
        left.occurred_at
            .cmp(&right.occurred_at)
            .then(left.key.cmp(&right.key))
    });
    events
}

#[cfg(test)]
mod tests {
    use super::{
        collect_issue_events, GithubBridgeEventKind, GithubIssue, GithubIssueComment, GithubUser,
    };

    fn sample_issue(author: &str, body: Option<&str>) -> GithubIssue {
        GithubIssue {
            id: 100,
            number: 42,
            title: "Issue".to_string(),
            body: body.map(|value| value.to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:10Z".to_string(),
            user: GithubUser {
                login: author.to_string(),
            },
            labels: Vec::new(),
            pull_request: None,
        }
    }

    #[test]
    fn unit_collect_issue_events_skips_blank_issue_body() {
        let issue = sample_issue("alice", Some("   "));
        let events = collect_issue_events(&issue, &[], "tau", true, true);
        assert!(events.is_empty());
    }

    #[test]
    fn functional_collect_issue_events_supports_created_and_edited_comments() {
        let issue = sample_issue("alice", Some("initial issue body"));
        let comments = vec![
            GithubIssueComment {
                id: 1,
                body: Some("first".to_string()),
                created_at: "2026-01-01T00:00:01Z".to_string(),
                updated_at: "2026-01-01T00:00:01Z".to_string(),
                user: GithubUser {
                    login: "bob".to_string(),
                },
            },
            GithubIssueComment {
                id: 2,
                body: Some("second edited".to_string()),
                created_at: "2026-01-01T00:00:02Z".to_string(),
                updated_at: "2026-01-01T00:10:02Z".to_string(),
                user: GithubUser {
                    login: "carol".to_string(),
                },
            },
        ];

        let events = collect_issue_events(&issue, &comments, "tau", true, true);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, GithubBridgeEventKind::Opened);
        assert_eq!(events[1].kind, GithubBridgeEventKind::CommentCreated);
        assert_eq!(events[2].kind, GithubBridgeEventKind::CommentEdited);
    }

    #[test]
    fn integration_collect_issue_events_applies_bot_and_edit_filters() {
        let issue = sample_issue("tau", Some("ignored bot body"));
        let comments = vec![
            GithubIssueComment {
                id: 10,
                body: Some("bot".to_string()),
                created_at: "2026-01-01T00:00:01Z".to_string(),
                updated_at: "2026-01-01T00:00:01Z".to_string(),
                user: GithubUser {
                    login: "tau".to_string(),
                },
            },
            GithubIssueComment {
                id: 11,
                body: Some("edited human".to_string()),
                created_at: "2026-01-01T00:00:02Z".to_string(),
                updated_at: "2026-01-01T00:02:02Z".to_string(),
                user: GithubUser {
                    login: "alice".to_string(),
                },
            },
        ];

        let events = collect_issue_events(&issue, &comments, "tau", true, false);
        assert!(events.is_empty());
    }

    #[test]
    fn regression_collect_issue_events_orders_by_time_then_key() {
        let issue = sample_issue("alice", None);
        let comments = vec![
            GithubIssueComment {
                id: 4,
                body: Some("b".to_string()),
                created_at: "2026-01-01T00:00:01Z".to_string(),
                updated_at: "2026-01-01T00:00:01Z".to_string(),
                user: GithubUser {
                    login: "bob".to_string(),
                },
            },
            GithubIssueComment {
                id: 3,
                body: Some("a".to_string()),
                created_at: "2026-01-01T00:00:01Z".to_string(),
                updated_at: "2026-01-01T00:00:01Z".to_string(),
                user: GithubUser {
                    login: "carol".to_string(),
                },
            },
        ];

        let events = collect_issue_events(&issue, &comments, "tau", false, true);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key, "issue-comment-created:3");
        assert_eq!(events[1].key, "issue-comment-created:4");
    }
}
