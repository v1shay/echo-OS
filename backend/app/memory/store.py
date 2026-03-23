from __future__ import annotations

import logging
import uuid
from typing import Any

import chromadb
from chromadb.utils.embedding_functions import OpenAIEmbeddingFunction

from app.config import Settings

logger = logging.getLogger(__name__)


class ChromaMemoryStore:
    def __init__(self, settings: Settings):
        self.settings = settings
        self.disabled = settings.enable_ollama_fallback
        self.client = chromadb.PersistentClient(path=str(settings.chroma_persist_directory))
        embedding_function = None
        if settings.openai_api_key and not self.disabled:
            embedding_function = OpenAIEmbeddingFunction(
                api_key=settings.openai_api_key.get_secret_value(),
                model_name=settings.embedding_model,
            )
        self.collection = self.client.get_or_create_collection(
            name=settings.chroma_collection_name,
            embedding_function=embedding_function,
        )

    def remember(self, text: str, metadata: dict[str, Any]) -> str:
        memory_id = str(uuid.uuid4())
        if self.disabled:
            return memory_id
        normalized_metadata = {key: value for key, value in metadata.items() if value is not None}
        try:
            self.collection.add(ids=[memory_id], documents=[text], metadatas=[normalized_metadata])
        except Exception as error:
            logger.warning("Skipping long-term memory write because Chroma storage failed: %s", error)
        return memory_id

    def recall(self, query: str, limit: int = 5, session_id: str | None = None) -> list[dict[str, Any]]:
        if self.disabled:
            return []
        filters = {"session_id": session_id} if session_id else None
        try:
            response = self.collection.query(query_texts=[query], n_results=limit, where=filters)
        except Exception as error:
            logger.warning("Skipping long-term memory recall because Chroma query failed: %s", error)
            return []
        documents = response.get("documents", [[]])[0]
        metadatas = response.get("metadatas", [[]])[0]
        distances = response.get("distances", [[]])[0]
        return [
            {
                "text": document,
                "metadata": metadata,
                "distance": distance,
            }
            for document, metadata, distance in zip(documents, metadatas, distances, strict=False)
        ]
