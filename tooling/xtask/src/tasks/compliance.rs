use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use semver::Version;

use crate::tasks::compliance::{
    checks::Reporter,
    git::{CommitsBetweenCommits, GitCommand, VersionTag},
    github::GitHubClient,
};

mod checks;
mod git;
mod github;
mod report;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ReleaseChannel {
    Stable,
    Preview,
}

impl ReleaseChannel {
    pub(crate) fn tag_suffix(&self) -> &'static str {
        match self {
            ReleaseChannel::Stable => "",
            ReleaseChannel::Preview => "-pre",
        }
    }
}

#[derive(Parser)]
pub struct ComplianceArgs {
    #[arg(value_parser = Version::parse)]
    // The version to be on the lookout for
    pub(crate) version: Version,
    #[arg(value_enum, default_value_t = ReleaseChannel::Stable)]
    // The release channel to check compliance for
    release_channel: ReleaseChannel,
    #[arg(long, default_value = "compliance-report.md")]
    // The markdown file to write the compliance report to
    report_path: PathBuf,
}

impl ComplianceArgs {
    pub(crate) fn version_tag(&self) -> VersionTag {
        VersionTag(format!(
            "v{version}{channel_suffix}",
            version = self.version,
            channel_suffix = self.release_channel.tag_suffix()
        ))
    }

    fn version_branch(&self) -> String {
        format!(
            "v{major}.{minor}.x",
            major = self.version.major,
            minor = self.version.minor
        )
    }
}

async fn check_compliance_impl(args: ComplianceArgs) -> Result<()> {
    let tag = args.version_tag();

    println!("Checking compliance for version: {}", tag.0);

    let mut commits = GitCommand::new(CommitsBetweenCommits(tag.clone())).run()?;

    //TODO REMOVE REMOVE REMOVE REMOVE!
    let _ = commits.split_off(60);

    println!("Found {} commits to check", commits.len());

    // let app_id = std::env::var("GITHUB_APP_ID").context("Missing GITHUB_APP_ID")?;
    // let key = std::env::var("GITHUB_APP_KEY").context("Missing GITHUB_APP_KEY")?;
    // let key = std::fs::read_to_string("zed-zippy-development.2026-03-30.private-key.pem")?;
    const APP_ID: u64 = 2008959;
    let key = std::fs::read_to_string("zed-zippy.2026-03-30.private-key.pem")?;
    let client = GitHubClient::for_app(APP_ID, key.as_ref()).await?;

    println!("Initialized GitHub client for app ID {APP_ID}");

    let report = Reporter::new(commits, client).generate_report().await?;

    let summary = report.summary();

    report.write_markdown(&args.report_path)?;

    println!("Wrote compliance report to {}", args.report_path.display());

    if summary.no_issues() {
        println!("No issues found, compliance check passed.");
        return Ok(());
    } else {
        println!("Issues found, compliance check failed.");
        return Err(anyhow::anyhow!(
            "Compliance check failed with {} issues",
            summary.errors
        ));
    }
}

pub fn check_compliance(args: ComplianceArgs) -> Result<()> {
    tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")
        .and_then(|handle| handle.block_on(check_compliance_impl(args)))
}
