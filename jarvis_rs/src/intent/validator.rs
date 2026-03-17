use serde::{Deserialize};
use anyhow::Result;
use super::{IntentObject, IntentType};

#[derive(Debug, Deserialize)]
pub struct OpenApplicationParams {
    pub app_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesParams {
    pub directory: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderParams {
    pub directory: String,
    pub folder_name: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteFileParams {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchWebParams {
    pub query: String,
}

#[derive(Debug)]
pub enum ValidatedParams {
    OpenApplication(OpenApplicationParams),
    ListFiles(ListFilesParams),
    CreateFolder(CreateFolderParams),
    DeleteFile(DeleteFileParams),
    SearchWeb(SearchWebParams),
}

pub fn validate_parameters(intent: &IntentObject) -> Result<ValidatedParams> {
    match intent.intent {
        IntentType::OpenApplication => {
            let params: OpenApplicationParams =
                serde_json::from_value(intent.parameters.clone())?;
            Ok(ValidatedParams::OpenApplication(params))
        }

        IntentType::ListFiles => {
            let params: ListFilesParams =
                serde_json::from_value(intent.parameters.clone())?;
            Ok(ValidatedParams::ListFiles(params))
        }

        IntentType::CreateFolder => {
            let params: CreateFolderParams =
                serde_json::from_value(intent.parameters.clone())?;
            Ok(ValidatedParams::CreateFolder(params))
        }

        IntentType::DeleteFile => {
            let params: DeleteFileParams =
                serde_json::from_value(intent.parameters.clone())?;
            Ok(ValidatedParams::DeleteFile(params))
        }

        IntentType::SearchWeb => {
            let params: SearchWebParams =
                serde_json::from_value(intent.parameters.clone())?;
            Ok(ValidatedParams::SearchWeb(params))
        }
    }
}
