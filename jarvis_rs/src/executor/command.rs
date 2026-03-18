use crate::intent::validator::ValidatedParams;
use anyhow::Result;
use std::process::Command;

pub fn execute(intent: ValidatedParams) -> Result<()> {
    match intent {
        ValidatedParams::OpenApplication(params) => {
            println!("Opening application: {}", params.app_name);

            Command::new("open")
                .arg("-a")
                .arg(params.app_name)
                .spawn()?;
        }

        ValidatedParams::ListFiles(params) => {
            println!("Listing files in: {}", params.directory);

            Command::new("ls").arg(params.directory).spawn()?;
        }

        ValidatedParams::CreateFolder(params) => {
            println!(
                "Creating folder: {}/{}",
                params.directory, params.folder_name
            );

            Command::new("mkdir")
                .arg(format!("{}/{}", params.directory, params.folder_name))
                .spawn()?;
        }

        ValidatedParams::DeleteFile(params) => {
            println!("Deleting file: {}", params.path);

            Command::new("rm").arg(params.path).spawn()?;
        }

        ValidatedParams::SearchWeb(params) => {
            println!("Searching web: {}", params.query);

            let url = format!("https://www.google.com/search?q={}", params.query);

            Command::new("open").arg(url).spawn()?;
        }
    }

    Ok(())
}
