"""Small synchronous client for Hermes's synchronous middleware contract."""

from __future__ import annotations

import json
import logging
from threading import Lock
import time
from typing import Any
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

from .config import Settings


logger = logging.getLogger(__name__)
_MAX_BODY_BYTES = 32 * 1024 * 1024


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: ANN001
        return None


class SidecarClient:
    """POST JSON to a loopback sidecar with a local circuit breaker."""

    def __init__(self, settings: Settings, *, opener: Any | None = None) -> None:
        self.settings = settings
        # Explicitly ignore HTTP(S)_PROXY. Prompt data must never leave the
        # literal loopback destination validated by Settings.
        self._opener = opener or build_opener(ProxyHandler({}), _NoRedirect())
        self._retry_after = 0.0
        self._lock = Lock()

    def optimize(self, envelope: dict[str, Any]) -> dict[str, Any] | None:
        now = time.monotonic()
        with self._lock:
            if now < self._retry_after:
                return None

        try:
            body = json.dumps(
                envelope, ensure_ascii=False, separators=(",", ":"), allow_nan=False
            ).encode("utf-8")
            if len(body) > _MAX_BODY_BYTES:
                raise ValueError("normalized request exceeds adapter body limit")
            request = Request(
                self.settings.endpoint + "/v1/optimize",
                data=body,
                method="POST",
                headers={
                    "content-type": "application/json",
                    "accept": "application/json",
                    "user-agent": "kendr-optimizer-hermes/0.1.4",
                },
            )
            with self._opener.open(
                request, timeout=self.settings.timeout_seconds
            ) as response:
                status = getattr(response, "status", 200)
                if not isinstance(status, int) or not 200 <= status < 300:
                    raise OSError(f"sidecar returned HTTP {status}")
                raw = response.read(_MAX_BODY_BYTES + 1)
                if len(raw) > _MAX_BODY_BYTES:
                    raise ValueError("sidecar response exceeds adapter body limit")
            decoded = json.loads(raw.decode("utf-8"))
            if not isinstance(decoded, dict):
                raise ValueError("sidecar response is not an object")
        except Exception as exc:
            with self._lock:
                self._retry_after = time.monotonic() + self.settings.backoff_seconds
            logger.debug(
                "KendrOptimizer sidecar unavailable; failing open (%s)",
                type(exc).__name__,
            )
            return None

        with self._lock:
            self._retry_after = 0.0
        return decoded
