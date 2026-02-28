# llm/schema.py

from pydantic import BaseModel, Field, ValidationError
from typing import Literal, Union


# ============================================================
# 1. Allowed Intent Types (MVP)
# ============================================================

INTENT_TYPES = [
    "open_application",
    "list_files",
    "create_folder",
    "delete_file",
    "search_web",
]


# ============================================================
# 2. Parameter Schemas Per Intent
# ============================================================

class OpenApplicationParams(BaseModel):
    app_name: str = Field(..., min_length=1)


class ListFilesParams(BaseModel):
    directory: str = Field(..., min_length=1)


class CreateFolderParams(BaseModel):
    directory: str = Field(..., min_length=1)
    folder_name: str = Field(..., min_length=1)


class DeleteFileParams(BaseModel):
    path: str = Field(..., min_length=1)


class SearchWebParams(BaseModel):
    query: str = Field(..., min_length=1)


# ============================================================
# 3. Canonical Intent Object
# ============================================================

class IntentObject(BaseModel):
    intent: Literal[
        "open_application",
        "list_files",
        "create_folder",
        "delete_file",
        "search_web",
    ]
    parameters: dict
    risk_level: Literal["low", "medium", "high"]
    requires_confirmation: bool


# ============================================================
# 4. Parameter Resolver
#    This enforces strict per-intent validation.
# ============================================================

def validate_intent_parameters(intent_obj: IntentObject):
    """
    Strictly validates parameters based on intent type.
    Raises ValidationError if invalid.
    Returns validated parameter model.
    """

    if intent_obj.intent == "open_application":
        return OpenApplicationParams(**intent_obj.parameters)

    elif intent_obj.intent == "list_files":
        return ListFilesParams(**intent_obj.parameters)

    elif intent_obj.intent == "create_folder":
        return CreateFolderParams(**intent_obj.parameters)

    elif intent_obj.intent == "delete_file":
        return DeleteFileParams(**intent_obj.parameters)

    elif intent_obj.intent == "search_web":
        return SearchWebParams(**intent_obj.parameters)

    else:
        raise ValueError(f"Unsupported intent: {intent_obj.intent}")