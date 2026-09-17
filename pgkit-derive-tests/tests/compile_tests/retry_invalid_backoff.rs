#[pgkit::retry(tries = 2, backoff = "quadratic")]
async fn bad_backoff() -> pgkit::errors::RepositoryResult<()> {
    Ok(())
}

fn main() {}
