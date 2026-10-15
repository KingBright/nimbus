"""Run launch scripts against isolated command stubs; never manage the real pet."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
STUB = r'''
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[1]).name
with open(os.environ["TEST_EVENTS"], "a") as output:
    output.write(json.dumps({"name": name, "args": sys.argv[2:], "cwd": os.getcwd(),
                             "http_proxy": os.environ.get("http_proxy"),
                             "https_proxy": os.environ.get("https_proxy")}) + "\n")
args = sys.argv[2:]
state = pathlib.Path(os.environ["TEST_STATE"])
if name == "cargo":
    if args[0] == "build":
        sys.exit(int(os.environ.get("BUILD_EXIT", "0")))
    if args[0] == "metadata":
        print(os.environ.get("METADATA_OUTPUT", json.dumps({"target_directory": os.environ["TEST_TARGET"]})))
        sys.exit(int(os.environ.get("METADATA_EXIT", "0")))
    sys.exit(99)
elif name == "launchctl":
    if args[0] == "print":
        sys.exit(0 if state.exists() else 1)
    if args[0] == "bootout":
        if os.environ.get("BOOTOUT_FAIL"):
            sys.exit(5)
        state.unlink(missing_ok=True)
        if os.environ.get("EXIT_ON_BOOTOUT"):
            pathlib.Path(os.environ["TEST_PROCESS_ALIVE"]).unlink(missing_ok=True)
    elif args[0] == "bootstrap":
        if os.environ.get("BOOTSTRAP_FAIL"):
            sys.exit(5)
        state.touch()
    else:
        sys.exit(99)
elif name == "pgrep":
    if os.environ.get("PGREP_FAIL"):
        sys.exit(2)
    if os.environ.get("PGREP_OUTPUT"):
        print(os.environ["PGREP_OUTPUT"])
    else:
        sys.exit(1)
elif name == "ps":
    if os.environ.get("PS_FAIL"):
        sys.exit(1)
    print(os.environ.get("PS_OUTPUT", "/another/checkout/release/nimbus"))
elif name == "kill":
    alive = pathlib.Path(os.environ["TEST_PROCESS_ALIVE"])
    if args[0] == "-0":
        sys.exit(0 if alive.exists() else 1)
    if args[0] == "-TERM":
        alive.unlink(missing_ok=True)
        sys.exit(1 if os.environ.get("TERM_EXIT_RACE") else 0)
    sys.exit(99)
else:
    sys.exit(99)
'''


class LaunchScriptsTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="nimbus-script-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.stub_source = self.root / "command_stub.py"
        self.stub_source.write_text(STUB)
        self.project = self.root / "project & space"
        self.project.mkdir()
        for script in ("run.sh", "deploy.sh"):
            shutil.copy2(REPO / script, self.project / script)
        self.mock_env = self.root / "mock-commands.sh"
        self.mock_env.write_text("".join(
            f'{command}() {{ "$TEST_PYTHON" "$TEST_STUB" {command} "$@"; }}\n'
            for command in ("cargo", "launchctl", "pgrep", "ps")))
        self.target = self.root / "build & output"
        self.pet = self.target / "release/nimbus"
        self.pet.parent.mkdir(parents=True)
        self.pet.symlink_to(shutil.which("true"))
        self.events = self.root / "events.jsonl"
        self.state = self.root / "loaded"
        self.user_home = self.root / "user"
        self.plist = self.user_home / "Library/LaunchAgents/fun.hackerlife.nimbus-pet.plist"
        self.env = dict(os.environ)
        for name in ("http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "all_proxy"):
            self.env.pop(name, None)
        self.env.update(BASH_ENV=str(self.mock_env), HOME=str(self.user_home),
                        TEST_EVENTS=str(self.events),
                        TEST_STATE=str(self.state), TEST_TARGET=str(self.target),
                        TEST_PYTHON=shutil.which("python3") or sys.executable,
                        TEST_STUB=str(self.stub_source))

    def run_script(self, script, *args, **overrides):
        return subprocess.run(["/bin/bash", str(self.project / script), *args],
                              cwd=self.root, env={**self.env, **overrides},
                              text=True, capture_output=True, timeout=30)

    def calls(self):
        return [json.loads(line) for line in self.events.read_text().splitlines()] if self.events.exists() else []

    def reset_calls(self):
        self.events.unlink(missing_ok=True)

    def install_old_service(self):
        self.plist.parent.mkdir(parents=True, exist_ok=True)
        self.plist.write_text("old configuration")
        self.state.touch()

    def mock_process_signals(self):
        # Override Bash's kill builtin only in race fixtures; send no real signals.
        with self.mock_env.open("a") as output:
            output.write('kill() { "$TEST_PYTHON" "$TEST_STUB" kill "$@"; }\n')
        alive = self.root / "process-alive"
        alive.touch()
        self.env.update(TEST_PROCESS_ALIVE=str(alive), PGREP_OUTPUT="987654321",
                        PS_OUTPUT=str(self.pet))

    def test_build_failure_preserves_existing_service(self):
        self.install_old_service()
        for script in ("run.sh", "deploy.sh"):
            with self.subTest(script=script):
                self.reset_calls()
                result = self.run_script(script, BUILD_EXIT="1")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual([call["name"] for call in self.calls()], ["cargo"])
                self.assertEqual(self.plist.read_text(), "old configuration")
                self.assertTrue(self.state.exists())

    def test_bad_metadata_or_missing_binary_never_stops_service(self):
        self.install_old_service()
        for script in ("run.sh", "deploy.sh"):
            for metadata in ("invalid json", '{"target_directory":"relative"}'):
                with self.subTest(script=script, metadata=metadata):
                    self.reset_calls()
                    result = self.run_script(script, METADATA_OUTPUT=metadata)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual([call["name"] for call in self.calls()], ["cargo", "cargo"])
        self.pet.unlink()
        for script in ("run.sh", "deploy.sh"):
            self.reset_calls()
            self.assertNotEqual(self.run_script(script).returncode, 0)
            self.assertEqual([call["name"] for call in self.calls()], ["cargo", "cargo"])
        self.assertTrue(self.state.exists())

    def test_foreground_runs_from_project_and_keeps_user_proxy(self):
        result = self.run_script("run.sh", http_proxy="http://user-proxy:1234", https_proxy="https://user-proxy:4321")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        self.assertEqual(calls[0]["args"], ["build", "--release", "--locked"])
        self.assertEqual(calls[1]["args"], ["metadata", "--offline", "--no-deps", "--format-version", "1"])
        self.assertIn("在此终端输入文字", result.stdout)
        self.assertTrue(all(call["cwd"] == str(self.project) for call in calls))
        self.assertTrue(all(call["http_proxy"] == "http://user-proxy:1234" for call in calls))
        self.assertTrue(all(call["https_proxy"] == "https://user-proxy:4321" for call in calls))

    def test_install_writes_escaped_plist_after_successful_build(self):
        self.install_old_service()
        result = self.run_script("deploy.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        self.assertEqual([call["name"] for call in calls[:2]], ["cargo", "cargo"])
        self.assertTrue(all(call["http_proxy"] is None and call["https_proxy"] is None for call in calls))
        with self.plist.open("rb") as source:
            config = plistlib.load(source)
        self.assertEqual(config["ProgramArguments"], [str(self.pet)])
        self.assertEqual(config["WorkingDirectory"], str(self.project))
        self.assertEqual(config["KeepAlive"], {"SuccessfulExit": False})
        self.assertTrue(self.state.exists())
        self.assertEqual(list(self.plist.parent.glob("*.plist.*")), [])

    def test_failed_bootstrap_reports_failure_without_legacy_fallback(self):
        result = self.run_script("deploy.sh", BOOTSTRAP_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("服务已注册", result.stdout)
        self.assertFalse(self.state.exists())
        self.assertNotIn("load", [call["args"][0] for call in self.calls() if call["name"] == "launchctl"])

    def test_failed_stop_preserves_old_configuration(self):
        self.install_old_service()
        for script in ("run.sh", "deploy.sh"):
            with self.subTest(script=script):
                self.reset_calls()
                result = self.run_script(script, BOOTOUT_FAIL="1")
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(self.state.exists())
                self.assertEqual(self.plist.read_text(), "old configuration")
                self.assertEqual(list(self.plist.parent.glob("*.plist.*")), [])

    def test_other_checkout_process_is_left_untouched(self):
        for script in ("run.sh", "deploy.sh"):
            with self.subTest(script=script):
                self.mock_process_signals()
                result = self.run_script(script, PS_OUTPUT="/another/checkout/release/nimbus")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(Path(self.env["TEST_PROCESS_ALIVE"]).exists())

    def test_process_inspection_failure_is_not_treated_as_no_process(self):
        self.install_old_service()
        for script in ("run.sh", "deploy.sh"):
            self.reset_calls()
            result = self.run_script(script, PGREP_FAIL="1")
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("launchctl", [call["name"] for call in self.calls()])
            self.assertTrue(self.state.exists())
            self.assertEqual(self.plist.read_text(), "old configuration")

    def test_path_inspection_failure_preserves_service(self):
        self.install_old_service()
        self.mock_process_signals()
        for script in ("run.sh", "deploy.sh"):
            self.reset_calls()
            result = self.run_script(script, PS_FAIL="1")
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("launchctl", [call["name"] for call in self.calls()])
            self.assertTrue(self.state.exists())
            self.assertEqual(self.plist.read_text(), "old configuration")

    def test_process_already_exited_after_bootout_is_success(self):
        for script in ("run.sh", "deploy.sh"):
            with self.subTest(script=script):
                self.reset_calls()
                self.install_old_service()
                self.mock_process_signals()
                result = self.run_script(script, EXIT_ON_BOOTOUT="1")
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = self.calls()
                names = [call["name"] for call in calls]
                self.assertLess(names.index("ps"), names.index("launchctl"))
                signals = [call["args"][0] for call in calls if call["name"] == "kill"]
                self.assertEqual(signals, ["-0"])

    def test_exit_between_probe_and_term_is_success(self):
        for script in ("run.sh", "deploy.sh"):
            with self.subTest(script=script):
                self.reset_calls()
                self.install_old_service()
                self.mock_process_signals()
                result = self.run_script(script, TERM_EXIT_RACE="1")
                self.assertEqual(result.returncode, 0, result.stderr)
                signals = [call["args"][0] for call in self.calls() if call["name"] == "kill"]
                self.assertEqual(signals, ["-0", "-TERM", "-0"])

    def test_background_immediate_exit_is_failure(self):
        self.pet.unlink()
        self.pet.symlink_to(shutil.which("false"))
        result = self.run_script("run.sh", "--daemon")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("Nimbus 已启动", result.stdout)

    def test_unknown_commands_and_restart_without_install_fail(self):
        for script in ("run.sh", "deploy.sh"):
            self.assertNotEqual(self.run_script(script, "typo").returncode, 0)
        self.assertNotEqual(self.run_script("deploy.sh", "restart").returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_uninstall_only_manages_registered_service(self):
        self.install_old_service()
        result = self.run_script("deploy.sh", "uninstall")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.state.exists())
        self.assertFalse(self.plist.exists())
        self.assertEqual([call["name"] for call in self.calls()], ["launchctl", "launchctl"])


if __name__ == "__main__":
    unittest.main()
