use std::process::Command;
use anyhow::Result;

pub fn open_application(app_name: &str) -> Result<()> {

    Command::new("open")
        .arg("-a")
        .arg(app_name)
        .spawn()?;

    Ok(())
}

pub fn list_files(path: &str) -> Result<String> {

    let output = Command::new("ls")
        .arg(path)
        .output()?;

    let result = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(result)
}

pub fn create_folder(name: &str) -> Result<()> {

    Command::new("mkdir")
        .arg(name)
        .spawn()?;

    Ok(())
}

pub fn delete_file(path: &str) -> Result<()> {

    Command::new("rm")
        .arg(path)
        .spawn()?;

    Ok(())
}

pub fn search_web(query: &str) -> Result<()> {

    let url = format!("https://www.google.com/search?q={}", query.replace(" ", "+"));

    Command::new("open")
        .arg(url)
        .spawn()?;

    Ok(())
}