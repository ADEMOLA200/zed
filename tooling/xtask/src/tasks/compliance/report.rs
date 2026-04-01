use std::{
    fmt,
    fs::{self, File},
    io::{BufWriter, Write},
    mem,
    ops::Not,
    path::Path,
    time::Duration,
};

use anyhow::Context as _;
use derive_more::Display;
use itertools::Itertools;
use octocrab::models::{
    Author,
    issues::Comment,
    pulls::{Review, ReviewState},
};

use crate::tasks::compliance::{
    git::{CommitDetails, CommitList},
    github::{CommitAuthor, GitHubClient, GithubLogin},
};

const ZED_ZIPPY_COMMENT_APPROVAL_PATTERN: &str = "@zed-zippy approved";
const PULL_REQUEST_BASE_URL: &str = "https://github.com/zed-industries/zed/pull";

#[derive(Debug)]
pub(crate) enum ReviewSuccess {
    ApprovingComment(Vec<Comment>),
    CoAuthored(Vec<CommitAuthor>),
    ExternalMergedContribution { merged_by: Box<Author> },
    PullRequestReviewed(Vec<Review>),
}

impl ReviewSuccess {
    fn reviewers(&self) -> String {
        let reviewers = match self {
            Self::CoAuthored(authors) => authors.iter().map(ToString::to_string).collect_vec(),
            Self::PullRequestReviewed(reviews) => reviews
                .iter()
                .filter_map(|review| review.user.as_ref())
                .map(|user| format!("@{}", user.login))
                .collect_vec(),
            Self::ApprovingComment(comments) => comments
                .iter()
                .map(|comment| format!("@{}", comment.user.login))
                .collect_vec(),
            Self::ExternalMergedContribution { merged_by } => vec![format!("@{}", merged_by.login)],
        };

        let reviewers = reviewers.into_iter().unique().collect_vec();
        if reviewers.is_empty() {
            "—".to_owned()
        } else {
            escape_markdown_table_text(&reviewers.join(", "))
        }
    }
}

impl fmt::Display for ReviewSuccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CoAuthored(_) => formatter.write_str("Co-authored by an organization member"),
            Self::PullRequestReviewed(_) => {
                formatter.write_str("Approved by an organization review")
            }
            Self::ApprovingComment(_) => {
                formatter.write_str("Approved by an organization approval comment")
            }
            Self::ExternalMergedContribution { .. } => {
                formatter.write_str("External merged contribution")
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum ReviewFailure {
    // todo: We could still query the GitHub API here to search for one
    NoPullRequestFound,
    Unreviewed,
    UnableToDetermineReviewer,
    Other(anyhow::Error),
}

impl fmt::Display for ReviewFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPullRequestFound => formatter.write_str("No pull request found"),
            Self::Unreviewed => formatter
                .write_str("No qualifying organization approval found for the pull request"),
            Self::UnableToDetermineReviewer => formatter.write_str("Could not determine reviewer"),
            Self::Other(error) => write!(formatter, "Failed to inspect review state: {error}"),
        }
    }
}

pub(crate) type ReviewResult = Result<ReviewSuccess, ReviewFailure>;

impl<E: Into<anyhow::Error>> From<E> for ReviewFailure {
    fn from(err: E) -> Self {
        Self::Other(anyhow::anyhow!(err))
    }
}

#[derive(Debug)]
struct ReportEntry {
    commit: CommitDetails,
    result: ReviewResult,
}

impl ReportEntry {
    fn issue_kind(&self) -> Option<IssueKind> {
        match self.result {
            Ok(_) => None,
            Err(ReviewFailure::Other(_)) => Some(IssueKind::Error),
            Err(_) => Some(IssueKind::NotReviewed),
        }
    }

    fn commit_cell(&self) -> String {
        let title = escape_markdown_link_text(self.commit.title());

        match self.commit.pr_number() {
            Some(pr_number) => format!("[{title}]({PULL_REQUEST_BASE_URL}/{pr_number})"),
            None => escape_markdown_table_text(self.commit.title()),
        }
    }

    fn pull_request_cell(&self) -> String {
        self.commit
            .pr_number()
            .map(|pr_number| format!("#{pr_number}"))
            .unwrap_or_else(|| "—".to_owned())
    }

    fn author_cell(&self) -> String {
        escape_markdown_table_text(&self.commit.author().to_string())
    }

    fn reviewers_cell(&self) -> String {
        match &self.result {
            Ok(success) => success.reviewers(),
            Err(_) => "—".to_owned(),
        }
    }

    fn reason_cell(&self) -> String {
        let reason = match &self.result {
            Ok(success) => success.to_string(),
            Err(failure) => failure.to_string(),
        };

        escape_markdown_table_text(&reason)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Report {
    entries: Vec<ReportEntry>,
}

#[derive(Debug, Default)]
struct ReportSummary {
    pull_requests: usize,
    reviewed: usize,
    not_reviewed: usize,
    errors: usize,
}

impl ReportSummary {
    fn from_entries(entries: &[ReportEntry]) -> Self {
        Self {
            pull_requests: entries
                .iter()
                .filter_map(|entry| entry.commit.pr_number())
                .unique()
                .count(),
            reviewed: entries.iter().filter(|entry| entry.result.is_ok()).count(),
            not_reviewed: entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.result,
                        Err(ReviewFailure::NoPullRequestFound | ReviewFailure::Unreviewed)
                    )
                })
                .count(),
            errors: entries
                .iter()
                .filter(|entry| matches!(entry.result, Err(ReviewFailure::Other(_))))
                .count(),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, PartialOrd, Ord)]
enum IssueKind {
    #[display("Error")]
    Error,
    #[display("Not reviewed")]
    NotReviewed,
}

impl Report {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, commit: CommitDetails, result: ReviewResult) {
        self.entries.push(ReportEntry { commit, result });
    }

    pub(crate) fn write_markdown(self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create parent directory for markdown report at {}",
                    path.display()
                )
            })?;
        }

        let Report { entries } = self;
        let summary = ReportSummary::from_entries(&entries);
        let (mut issues, successes): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|entry| entry.result.is_err());

        issues.sort_by_key(|entry| entry.issue_kind().unwrap_or(IssueKind::NotReviewed));

        let file = File::create(path)
            .with_context(|| format!("Failed to create markdown report at {}", path.display()))?;
        let mut writer = BufWriter::new(file);

        writeln!(writer, "# Compliance report")?;
        writeln!(writer)?;
        writeln!(writer, "## Overview")?;
        writeln!(writer)?;
        writeln!(writer, "- PRs: {}", summary.pull_requests)?;
        writeln!(writer, "- Reviewed: {}", summary.reviewed)?;
        writeln!(writer, "- Not reviewed: {}", summary.not_reviewed)?;
        writeln!(writer, "- Errors: {}", summary.errors)?;
        writeln!(writer)?;

        write_issue_table(&mut writer, &issues)?;
        write_success_table(&mut writer, &successes)?;

        writer
            .flush()
            .with_context(|| format!("Failed to flush markdown report to {}", path.display()))
    }
}

fn write_issue_table(writer: &mut impl Write, issues: &[ReportEntry]) -> std::io::Result<()> {
    writeln!(writer, "## Errors and unreviewed commits")?;
    writeln!(writer)?;

    if issues.is_empty() {
        writeln!(writer, "No errors or unreviewed commits found.")?;
        writeln!(writer)?;
        return Ok(());
    }

    writeln!(
        writer,
        "| Commit | PR | Author | Reviewers | Outcome | Reason |"
    )?;
    writeln!(writer, "| --- | --- | --- | --- | --- | --- |")?;

    for entry in issues {
        let issue_kind = entry.issue_kind().unwrap_or(IssueKind::NotReviewed);
        writeln!(
            writer,
            "| {} | {} | {} | {} | {} | {} |",
            entry.commit_cell(),
            entry.pull_request_cell(),
            entry.author_cell(),
            entry.reviewers_cell(),
            issue_kind,
            entry.reason_cell(),
        )?;
    }

    writeln!(writer)?;
    Ok(())
}

fn write_success_table(
    writer: &mut impl Write,
    successful_entries: &[ReportEntry],
) -> std::io::Result<()> {
    writeln!(writer, "## Successful commits")?;
    writeln!(writer)?;

    if successful_entries.is_empty() {
        writeln!(writer, "No successful commits found.")?;
        writeln!(writer)?;
        return Ok(());
    }

    writeln!(writer, "| Commit | PR | Author | Reviewers | Reason |")?;
    writeln!(writer, "| --- | --- | --- | --- | --- |")?;

    for entry in successful_entries {
        writeln!(
            writer,
            "| {} | {} | {} | {} | {} |",
            entry.commit_cell(),
            entry.pull_request_cell(),
            entry.author_cell(),
            entry.reviewers_cell(),
            entry.reason_cell(),
        )?;
    }

    writeln!(writer)?;
    Ok(())
}

fn escape_markdown_link_text(input: &str) -> String {
    escape_markdown_table_text(input)
        .replace('[', r"\[")
        .replace(']', r"\]")
}

fn escape_markdown_table_text(input: &str) -> String {
    input
        .replace('\\', r"\\")
        .replace('|', r"\|")
        .replace('\r', "")
        .replace('\n', "<br>")
}

pub(crate) struct Reporter {
    commits: CommitList,
    github_client: GitHubClient,
}

impl Reporter {
    pub fn new(commits: CommitList, github_client: GitHubClient) -> Self {
        Self {
            commits,
            github_client,
        }
    }

    async fn check_commit(
        &mut self,
        commit: &CommitDetails,
    ) -> Result<ReviewSuccess, ReviewFailure> {
        // Check co-authors first
        if commit.co_authors().is_some()
            && let Some(commit_authors) = self
                .github_client
                .get_commit_co_authors([commit.sha()])
                .await?
                .get(commit.sha())
                .and_then(|authors| authors.co_authors())
        {
            let mut org_co_authors = Vec::new();
            for co_author in commit_authors {
                if let Some(github_login) = co_author.user()
                    && self
                        .github_client
                        .check_org_membership(github_login)
                        .await?
                {
                    org_co_authors.push(co_author.clone());
                }
            }

            if org_co_authors.is_empty().not() {
                return Ok(ReviewSuccess::CoAuthored(org_co_authors));
            }
        }

        let Some(pr_number) = commit.pr_number() else {
            return Err(ReviewFailure::NoPullRequestFound);
        };

        let pull_request = self.github_client.get_pull_request(pr_number).await?;

        if let Some(user) = pull_request.user
            && self
                .github_client
                .check_org_membership(&GithubLogin::new(user.login))
                .await?
                .not()
        {
            if let Some(merged_by) = pull_request.merged_by {
                return Ok(ReviewSuccess::ExternalMergedContribution { merged_by });
            } else {
                return Err(ReviewFailure::UnableToDetermineReviewer);
            }
        }

        let other_comments = self
            .github_client
            .get_pr_reviews(pr_number)
            .await?
            .take_items();

        if !other_comments.is_empty() {
            let mut org_approving_reviews = Vec::new();
            for review in other_comments {
                if let Some(github_login) = review.user.as_ref()
                    && review
                        .state
                        .is_some_and(|state| state == ReviewState::Approved)
                    && self
                        .github_client
                        .check_org_membership(&GithubLogin::new(github_login.login.clone()))
                        .await?
                {
                    org_approving_reviews.push(review);
                }
            }

            if org_approving_reviews.is_empty().not() {
                return Ok(ReviewSuccess::PullRequestReviewed(org_approving_reviews));
            }
        }

        let other_comments = self
            .github_client
            .get_pr_comments(pr_number)
            .await?
            .take_items();

        if !other_comments.is_empty() {
            let mut org_approving_comments = Vec::new();

            for comment in other_comments {
                if comment
                    .body
                    .as_ref()
                    .is_some_and(|body| body.contains(ZED_ZIPPY_COMMENT_APPROVAL_PATTERN))
                    && self
                        .github_client
                        .check_org_membership(&GithubLogin::new(comment.user.login.clone()))
                        .await?
                {
                    org_approving_comments.push(comment);
                }
            }

            if org_approving_comments.is_empty().not() {
                return Ok(ReviewSuccess::ApprovingComment(org_approving_comments));
            }
        }

        Err(ReviewFailure::Unreviewed)
    }

    pub(crate) async fn generate_report(&mut self) -> anyhow::Result<Report> {
        let mut report = Report::new();

        let commits_to_check = mem::take(&mut self.commits);
        let total_commits = commits_to_check.len();

        for (i, commit) in commits_to_check.into_iter().enumerate() {
            println!(
                "Checking commit {:?} ({current}/{total})",
                commit.sha().as_str().split_at(8).0,
                current = i + 1,
                total = total_commits
            );

            let review_result = self.check_commit(&commit).await;

            report.add(commit, review_result);

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(report)
    }
}
