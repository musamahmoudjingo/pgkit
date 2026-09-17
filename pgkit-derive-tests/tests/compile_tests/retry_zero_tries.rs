#[pgkit::retry(tries = 0)]
async fn zero_tries() -> pgkit::errors::RepositoryResult<()> {
    Ok(())
}

fn main() {}
