mod client;
mod tools;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

use crate::{client::GitLabClient, tools::GitLabMcp};

#[tokio::main]
async fn main() -> Result<()> {
    let client = GitLabClient::from_env()?;
    let service = GitLabMcp::new(client).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
