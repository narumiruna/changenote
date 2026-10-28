"""Check the opt-in high-level composite's setup order and input boundaries."""

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


class ReleaseMetadata(unittest.TestCase):
    def setUp(self):
        self.action = yaml.load((ROOT / "release/action.yml").read_text(), Loader=yaml.BaseLoader)
        self.steps = self.action["runs"]["steps"]

    def step(self, name):
        return next(step for step in self.steps if step.get("name") == name)

    def test_defaults_and_output_forwarding(self):
        self.assertEqual(self.action["runs"]["using"], "composite")
        defaults = {
            "operation": "auto", "github-token": "${{ github.token }}",
            "base-branch": "", "expected-release-commit": "", "retry": "false",
            "node-version": "lts/*", "npm-version": "", "rust-toolchain": "stable",
            "uv-version": "", "commit-message": "chore(release): version packages",
            "pr-title": "chore(release): version packages",
        }
        self.assertEqual({key: value["default"] for key, value in self.action["inputs"].items()}, defaults)
        self.assertEqual(set(self.action["outputs"]), {
            "release-commit", "releases", "published", "published-packages",
            "publication-plan", "publication-needed", "ready", "has-notes",
            "pr-number", "pr-operation",
        })
        for key, value in self.action["outputs"].items():
            self.assertEqual(value["value"], "${{ steps.release.outputs." + key + " }}")
        # This Action is opt-in. The original entry point still manages only PRs.
        root = yaml.load((ROOT / "action.yml").read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(set(root["outputs"]), {"has-notes", "pr-number", "pr-operation"})
        self.assertIn("version.sh", root["runs"]["steps"][0]["run"])

    def test_setup_order_and_cache_hit_activation(self):
        labels = [step.get("name", step.get("uses")) for step in self.steps]
        self.assertEqual(labels, [
            "Validate release inputs", "Identify trusted CLI source", "actions/cache@v5",
            "dtolnay/rust-toolchain@stable", "Build trusted CLI", "Verify and activate CLI",
            "Discover release environments", "actions/setup-node@v7", "Install selected npm version",
            "astral-sh/setup-uv@v7", "Populate offline package caches", "Run release operation",
        ])
        cache = next(step for step in self.steps if step.get("id") == "cache")
        self.assertEqual(cache["with"], {
            "path": "${{ steps.cli.outputs.directory }}", "key": "${{ steps.cli.outputs.key }}",
        })
        self.assertEqual(self.step("Build trusted CLI")["if"], "steps.cache.outputs.cache-hit != 'true'")
        self.assertNotIn("if", self.step("Verify and activate CLI"))
        rust = next(step for step in self.steps if step.get("uses", "").startswith("dtolnay/"))
        self.assertEqual(rust["with"]["toolchain"], "${{ inputs.rust-toolchain }}")
        self.assertNotIn("if", rust)  # The trusted CLI always needs Rust on a cache miss.
        for name, script, operation in (
            ("Validate release inputs", "release.sh", "validate"),
            ("Identify trusted CLI source", "install-cli.sh", "key"),
            ("Build trusted CLI", "install-cli.sh", "install"),
            ("Verify and activate CLI", "install-cli.sh", "activate"),
            ("Discover release environments", "release-environment.sh", "discover"),
            ("Populate offline package caches", "release-environment.sh", "warm"),
            ("Run release operation", "release.sh", "run"),
        ):
            self.assertEqual(self.step(name)["run"], f'bash "$ACTION_PATH/../scripts/action/{script}" {operation}')

    def test_config_based_environment_conditions_and_versions(self):
        node = next(step for step in self.steps if step.get("uses", "").startswith("actions/setup-node@"))
        uv = next(step for step in self.steps if step.get("uses", "").startswith("astral-sh/setup-uv@"))
        self.assertEqual(node["if"], "steps.environment.outputs.npm == 'true'")
        self.assertEqual(node["with"], {
            "node-version": "${{ inputs.node-version }}", "package-manager-cache": "false",
        })
        self.assertEqual(uv["if"], "steps.environment.outputs.uv == 'true'")
        self.assertEqual(uv["with"]["version"], "${{ inputs.uv-version }}")
        npm = self.step("Install selected npm version")
        self.assertEqual(npm["if"], "steps.environment.outputs.npm == 'true' && inputs.npm-version != ''")
        self.assertEqual(npm["env"], {"INPUT_NPM_VERSION": "${{ inputs.npm-version }}"})
        self.assertEqual(npm["run"], 'npm install --global --ignore-scripts --no-audit --no-fund "npm@$INPUT_NPM_VERSION"')
        self.assertNotIn("if", self.step("Populate offline package caches"))

    def test_expressions_enter_shell_only_through_environment(self):
        for step in self.steps:
            if "run" not in step:
                continue
            with self.subTest(step=step["name"]):
                self.assertEqual(step["shell"], "bash")
                self.assertNotIn("${{", step["run"])
                if "ACTION_PATH" in step["env"]:
                    self.assertEqual(step["env"]["ACTION_PATH"], "${{ github.action_path }}")
        validate = self.step("Validate release inputs")
        for key in ("operation", "expected-release-commit", "retry", "npm-version", "rust-toolchain"):
            self.assertEqual(validate["env"]["INPUT_" + key.upper().replace("-", "_")], "${{ inputs." + key + " }}")
        self.assertNotIn("GH_TOKEN", validate["env"])
        for name in ("Identify trusted CLI source", "Build trusted CLI", "Verify and activate CLI"):
            self.assertNotIn("GH_TOKEN", self.step(name)["env"])
            self.assertEqual(self.step(name)["env"]["INPUT_RUST_TOOLCHAIN"], "${{ inputs.rust-toolchain }}")
        for name in ("Discover release environments", "Populate offline package caches", "Run release operation"):
            env = self.step(name)["env"]
            self.assertEqual(env["INPUT_OPERATION"], "${{ steps.inputs.outputs.operation }}")
            self.assertEqual(env["GH_TOKEN"], "${{ inputs.github-token }}")
            self.assertEqual(env["INPUT_BASE_BRANCH"], "${{ inputs.base-branch }}")
            self.assertEqual(env["INPUT_EXPECTED_RELEASE_COMMIT"], "${{ inputs.expected-release-commit }}")
        for name in ("Discover release environments", "Populate offline package caches"):
            self.assertEqual(self.step(name)["env"]["INPUT_RUST_TOOLCHAIN"], "${{ inputs.rust-toolchain }}")
        self.assertEqual(self.step("Populate offline package caches")["id"], "warm")
        run = self.step("Run release operation")
        self.assertEqual(run["env"]["INPUT_PR_ENVIRONMENT_READY"], "${{ steps.environment.outputs.pr-environment-ready != 'false' && steps.warm.outputs.pr-environment-ready != 'false' }}")
        self.assertEqual(run["id"], "release")
        for key in ("retry", "npm-version", "rust-toolchain", "commit-message", "pr-title"):
            self.assertEqual(run["env"]["INPUT_" + key.upper().replace("-", "_")], "${{ inputs." + key + " }}")


if __name__ == "__main__":
    unittest.main()
