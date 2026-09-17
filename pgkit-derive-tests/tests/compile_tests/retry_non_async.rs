#[pgkit::retry(tries = 2)]
fn not_async() -> pgkit::errors::RepositoryResult<()> {
    Ok(())
}

fn main() {}
