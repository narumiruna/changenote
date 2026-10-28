"""Check crates.io retries without network access or Cargo uploads."""

import importlib.util
import io
import json
import os
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch
from urllib.error import HTTPError, URLError

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/action/publish-crate.py"
spec = importlib.util.spec_from_file_location("crate_publication", SCRIPT)
publication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publication)


class CratePublication(unittest.TestCase):
    def setUp(self):
        self.root = Path("/verified/release")
        self.version = "1.2.3"
        self.url = "https://crates.io/api/v1/crates/changenote/1.2.3"
        self.open = self.enterContext(patch.object(publication, "urlopen"))
        self.cargo = self.enterContext(patch.object(publication.subprocess, "run"))
        self.enterContext(
            patch.dict(os.environ, {"CARGO_REGISTRY_TOKEN": "test-token"})
        )

    def response(self, **fields):
        version = {"crate": "changenote", "num": self.version, "yanked": False}
        version.update(fields)
        self.open.return_value.__enter__.return_value = io.BytesIO(
            json.dumps({"version": version}).encode()
        )

    def missing(self):
        error = HTTPError(self.url, 404, "Not Found", {}, None)
        self.addCleanup(error.close)
        self.open.side_effect = error

    def test_missing_version_publishes_locked_package(self):
        self.missing()
        publication.publish(self.root, self.version)
        self.cargo.assert_called_once_with(
            [
                "cargo",
                "publish",
                "--locked",
                "--registry",
                "crates-io",
                "--package",
                "changenote",
                "--allow-dirty",
            ],
            cwd=self.root,
            check=True,
        )

    def test_existing_version_is_skipped_without_credentials(self):
        self.response()
        with patch.dict(os.environ, {"CARGO_REGISTRY_TOKEN": ""}):
            publication.publish(self.root, self.version)
        self.cargo.assert_not_called()
        self.open.assert_called_once()
        request = self.open.call_args.args[0]
        self.assertEqual(request.full_url, self.url)
        self.assertIn("changenote", request.get_header("User-agent"))
        self.assertNotIn("Authorization", request.headers)
        self.assertEqual(self.open.call_args.kwargs, {"timeout": 30})

    def test_invalid_or_yanked_versions_are_rejected(self):
        for fields in (
            {"crate": "other"},
            {"num": "1.2.4"},
            {"yanked": True},
            {"yanked": None},
            {"yanked": 0},
        ):
            with self.subTest(fields=fields):
                self.response(**fields)
                with self.assertRaises(ValueError):
                    publication.publish(self.root, self.version)
                self.cargo.assert_not_called()

    def test_invalid_response_fails_without_upload(self):
        for body in (b"not json", b"{}"):
            with self.subTest(body=body):
                self.open.return_value.__enter__.return_value = io.BytesIO(body)
                with self.assertRaises((ValueError, KeyError)):
                    publication.publish(self.root, self.version)
                self.cargo.assert_not_called()

    def test_api_errors_fail_without_upload(self):
        errors = [
            HTTPError(self.url, status, "API failure", {}, None)
            for status in (401, 403, 429, 500, 503)
        ] + [URLError("network unavailable"), TimeoutError("request timed out")]
        for error in errors:
            if isinstance(error, HTTPError):
                self.addCleanup(error.close)
            with self.subTest(error=error):
                self.open.side_effect = error
                with self.assertRaises(type(error)):
                    publication.publish(self.root, self.version)
                self.cargo.assert_not_called()

    def test_missing_token_fails_before_upload(self):
        self.missing()
        with patch.dict(os.environ, {"CARGO_REGISTRY_TOKEN": ""}):
            with self.assertRaisesRegex(ValueError, "CARGO_REGISTRY_TOKEN"):
                publication.publish(self.root, self.version)
        self.cargo.assert_not_called()

    def test_cargo_failure_is_not_ignored(self):
        self.missing()
        self.cargo.side_effect = subprocess.CalledProcessError(1, "cargo publish")
        with self.assertRaises(subprocess.CalledProcessError):
            publication.publish(self.root, self.version)
        self.cargo.assert_called_once()


if __name__ == "__main__":
    unittest.main()
