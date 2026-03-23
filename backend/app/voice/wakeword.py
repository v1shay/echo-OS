from __future__ import annotations

from pathlib import Path

import numpy as np

from app.config import Settings


class AcousticWakeWordDetector:
    def __init__(self, settings: Settings):
        self.settings = settings
        self.enabled = False
        self.model = None

        if settings.wakeword_provider != "openwakeword" or not settings.wakeword_model_path:
            return

        model_path = Path(settings.wakeword_model_path)
        if not model_path.exists():
            return

        try:
            from openwakeword.model import Model

            self.model = Model(wakeword_models=[str(model_path)])
            self.enabled = True
        except Exception:
            self.enabled = False

    def detect(self, pcm16_mono: bytes) -> bool:
        if not self.enabled or not self.model:
            return False
        samples = np.frombuffer(pcm16_mono, dtype=np.int16)
        scores = self.model.predict(samples)
        if isinstance(scores, dict):
            top_score = max(scores.values(), default=0.0)
        else:
            top_score = 0.0
        return top_score >= self.settings.wakeword_threshold
