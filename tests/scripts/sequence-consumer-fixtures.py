#!/usr/bin/env python3
"""Replay real composed sequence graphs with harmless leaves and injected failures.

Reads consumers only through Effigy config/resolve APIs. All generated manifests,
scripts, trace files and homes live inside one fresh temporary directory.
"""
import argparse
import copy
import hashlib
import fnmatch
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile

try:
    import tomllib
except ModuleNotFoundError:
    from pip._vendor import tomli as tomllib


def invoke(binary, args, cwd, env=None):
    return subprocess.run([str(binary), *args], cwd=cwd, env=env,
                          capture_output=True, text=True, timeout=90)


def query(binary, args, cwd):
    completed = invoke(binary, ["--json", *args], cwd)
    payload = json.loads(completed.stdout)
    if completed.returncode or not payload.get("ok"):
        raise RuntimeError(f"{args}: {payload.get('error') or completed.stderr}")
    return payload["result"]


def toml(value):
    if isinstance(value, dict):
        return "{" + ", ".join(f"{json.dumps(key)} = {toml(item)}" for key, item in value.items()) + "}"
    if isinstance(value, list):
        return "[" + ", ".join(toml(item) for item in value) + "]"
    return json.dumps(value)


class Graph:
    def __init__(self, binary, source, destination):
        self.binary, self.source, self.destination = binary, source, destination
        inventory = query(binary, ["tasks"], source)
        self.catalogs = {item["alias"]: item for item in inventory["catalogs"]}
        self.builtins = {item["task"] for item in inventory["builtin_tasks"]}
        aliases = {item["manifest"]: item["alias"] for item in inventory["catalogs"]}
        sequence_keys = {(aliases[item["manifest"]], item["task"].rsplit("/", 1)[-1]) for item in inventory["catalog_tasks"]
                         if (item.get("run") or "").startswith("<sequence:")}
        self.original, self.names, self.tasks, self.leaves = {}, {}, {}, {}
        self.profiles, self.entries, self.provenance = {}, [], []
        self.edges, self.serial = {}, 0
        for alias, catalog in self.catalogs.items():
            args = ["config", "--inspect", "--path", "tasks"]
            if Path(catalog["root"]) != Path.cwd():
                args = ["--repo", catalog["root"], *args]
            inspected = query(binary, args, Path.cwd())
            definitions = inspected["selected_value"]["value"]
            bootstrap = tomllib.loads(inspected["effective_manifest"]).get("bootstrap", {})
            for field in ("run", "setup"):
                steps = bootstrap.get(field)
                if isinstance(steps, list) and steps:
                    name = f"<bootstrap.{field}>"
                    definitions[name] = {"run": [{"task": step} if isinstance(step, str) else step for step in steps]}
                    sequence_keys.add((alias, name))
            for name, definition in definitions.items():
                key = (alias, name)
                self.names[key] = f"fixture:{len(self.names)}"
                self.original[key] = definition
        for key, definition in self.original.items():
            run = definition.get("run") if isinstance(definition, dict) else definition
            if run is None and isinstance(definition, dict) and any(k in definition for k in ("task", "rhai")):
                run = [definition]
            if isinstance(run, dict):
                run = [run]
            if key in sequence_keys and not isinstance(run, list):
                run = [{field: value for field, value in definition.items()
                        if field in ("run", "task", "rhai", "draft", "env", "env_file", "id", "timeout_ms")}]
            steps = [self.step(key[0], step) for step in run] if isinstance(run, list) else self.leaf("command")
            task = {"run": steps, "run_in": "host"}
            if isinstance(definition, dict) and definition.get("lock"):
                task["lock"] = definition["lock"]
            self.tasks[self.names[key]] = task
            if isinstance(run, list):
                self.entries.append((key, self.names[key]))
                self.provenance.append({"catalog": key[0], "task": key[1], "steps": len(run),
                                        "sha256": hashlib.sha256(json.dumps(definition, sort_keys=True).encode()).hexdigest()})
        if len(self.entries) != len(sequence_keys):
            raise RuntimeError("fixture inventory differs from canonical sequence inventory")
        # Setup is a sequence as well. Its container timing/journals remain
        # deferred; replay its exact ordered structure through the host engine.
        for key, definition in self.original.items():
            if not isinstance(definition, dict):
                continue
            groups = [definition.get("concurrent", [])]
            groups.extend(profile.get("concurrent", []) for profile in definition.get("profiles", {}).values())
            for group in groups:
                for process in group:
                    setup = process.get("setup", [])
                    if not setup:
                        continue
                    name = f"fixture:setup:{len(self.entries)}"
                    self.tasks[name] = {"run": [self.step(key[0], step) for step in setup], "run_in": "host"}
                    label = (key[0], key[1] + "/setup/" + process.get("name", str(len(self.entries))))
                    self.entries.append((label, name))
                    self.provenance.append({"catalog": label[0], "task": label[1], "steps": len(setup),
                                            "sha256": hashlib.sha256(json.dumps(setup, sort_keys=True).encode()).hexdigest()})
        manifest = "[tasks]\n" + "\n".join(f"{json.dumps(name)} = {toml(value)}" for name, value in self.tasks.items())
        if self.profiles:
            manifest += "\n[env]\n" + "\n".join(f"{json.dumps(name)} = \"fixture\"" for name in self.profiles)
        (destination / "effigy.toml").write_text(manifest + "\n")

    def leaf(self, kind):
        identity = f"leaf-{self.serial}"
        self.serial += 1
        command = ("n=0; if test -f \"$FIXTURE_COUNTER\"; then read n < \"$FIXTURE_COUNTER\"; fi; "
                   "n=$((n + 1)); printf '%s\\n' \"$n\" > \"$FIXTURE_COUNTER\"; "
                   f"printf '%s\\n' {identity} >> \"$FIXTURE_TRACE\"; "
                   "if test \"$FIXTURE_FAIL_AT\" = \"$n\"; then exit 7; fi")
        self.leaves[command] = identity
        if kind == "rhai":
            path = f"scripts/{identity}.rhai"
            target = self.destination / path
            target.parent.mkdir(exist_ok=True)
            target.write_text(f'let result = process::run("sh", ["-c", {json.dumps(command)}]);\n'
                              'if !result["success"] { throw("injected fixture failure"); }\n')
            self.leaves[path] = identity
            return path
        return command

    def reference(self, alias, raw):
        selector = shlex.split(raw)[0]
        cache = (alias, selector)
        if cache not in self.edges:
            # The engine identifies builtin references before task selection.
            # Preserve the task-reference step, with a harmless stub leaf.
            if selector.rsplit("/", 1)[-1] in self.builtins:
                name = f"fixture:external:{len(self.edges)}"
                self.tasks[name] = {"run": self.leaf("command"), "run_in": "host"}
                self.edges[cache] = name
                return name
            # Match the engine's owner-catalog default, then ask its resolver.
            qualified = selector if "/" in selector else f"{alias}/{selector}"
            resolved = query(self.binary, ["tasks", "--resolve", qualified], self.source)["resolve"]
            if resolved.get("status") != "ok":
                raise RuntimeError(f"unresolved real consumer reference {alias}/{raw}: {resolved}")
            target = (resolved.get("catalog"), resolved.get("task"))
            if resolved.get("route") == "task" and target in self.names:
                self.edges[cache] = self.names[target]
            else:
                name = f"fixture:external:{len(self.edges)}"
                self.tasks[name] = {"run": self.leaf("command"), "run_in": "host"}
                self.edges[cache] = name
        return self.edges[cache]

    def step(self, alias, original):
        if isinstance(original, str):
            return ("task: " + self.reference(alias, original[5:].strip())) if original.startswith("task:") else self.leaf("command")
        step = {key: copy.deepcopy(value) for key, value in original.items() if key in ("id", "timeout_ms", "env", "env_file")}
        if isinstance(step.get("env"), str):
            name = f"profile{len(self.profiles)}"
            self.profiles[name] = True
            step["env"] = name
        elif isinstance(step.get("env"), dict):
            step["env"] = {key: "fixture" for key in step["env"]}
        if "env_file" in step:
            files = step["env_file"] if isinstance(step["env_file"], list) else [step["env_file"]]
            replacements = []
            for _ in files:
                path = f"fixture-{self.serial}.env"
                self.serial += 1
                (self.destination / path).write_text("FIXTURE_ENV=fixture\n")
                replacements.append(path)
            step["env_file"] = replacements if isinstance(step["env_file"], list) else replacements[0]
        if "task" in original:
            step["task"] = self.reference(alias, original["task"])
        elif "draft" in original:
            raise RuntimeError("consumer draft reference requires a draft fixture")
        elif "rhai" in original:
            step["rhai"] = self.leaf("rhai")
        elif "run" in original:
            step["run"] = self.leaf("command")
        return step

    def trace(self, name, ancestors=()):
        if name in ancestors:
            raise RuntimeError(f"consumer reference cycle: {ancestors + (name,)}")
        run = self.tasks[name]["run"]
        if isinstance(run, str):
            return [self.leaves[run]], [1]
        trace, failures = [], []
        for step in run:
            nested_points = []
            if isinstance(step, str) and step.startswith("task:"):
                part, nested_points = self.trace(step[5:].strip(), ancestors + (name,))
            elif isinstance(step, str):
                part = [self.leaves[step]]
            elif "task" in step:
                part, nested_points = self.trace(step["task"], ancestors + (name,))
            else:
                part = [self.leaves[step[key]]] if (key := next((key for key in ("run", "rhai") if key in step), None)) else []
            if part:
                failures.extend(len(trace) + point for point in (nested_points or [1]))
            trace.extend(part)
        return trace, failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--root", type=Path, action="append", required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--select", action="append", help="target qualified source sequence/setup patterns; defaults to all")
    args = parser.parse_args()
    working = Path(tempfile.mkdtemp(prefix="effigy-consumer-sequences-"))
    binaries = {}
    for label, source in (("candidate", args.candidate), ("baseline", args.baseline)):
        binary = working / label
        shutil.copy2(source, binary)
        subprocess.run([str(binary), "--version"], check=True, capture_output=True, timeout=90)
        binaries[label] = binary
    report = {"fixture_root": str(working), "families": []}
    failures = []
    for index, source in enumerate(args.root):
        destination = working / f"family-{index}"
        destination.mkdir()
        graph = Graph(binaries["candidate"], source.resolve(), destination)
        family = {"source": str(source), "definitions": graph.provenance, "cases": []}
        selected = [(key, name) for key, name in graph.entries
                    if not args.select or any(fnmatch.fnmatchcase("/".join(key), pattern) for pattern in args.select)]
        if not selected:
            raise RuntimeError(f"no requested sequence fixtures in {source}")
        family["selected"] = ["/".join(key) for key, _ in selected]
        report["families"].append(family)
        for key, name in selected:
            expected, points = graph.trace(name)
            for point in [0, *sorted(set(points))]:
                results = {}
                for label, binary in binaries.items():
                    for json_mode in (False, True):
                        case = Path(tempfile.mkdtemp(prefix="case-", dir=destination))
                        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(case),
                               "EFFIGY_EXECUTABLE": str(binary), "NO_COLOR": "1",
                               "FIXTURE_COUNTER": str(case / "counter"), "FIXTURE_TRACE": str(case / "trace"),
                               "FIXTURE_FAIL_AT": str(point)}
                        completed = invoke(binary, (["--json"] if json_mode else []) + [name], destination, env)
                        trace = (case / "trace").read_text().splitlines() if (case / "trace").exists() else []
                        result = {"exit": completed.returncode, "trace": trace}
                        results[f"{label}-{'json' if json_mode else 'text'}"] = result
                        wanted = expected[:point] if point else expected
                        if label == "candidate" and (trace != wanted or (completed.returncode == 0) != (point == 0)):
                            failures.append({"catalog": key[0], "task": key[1], "point": point, "json": json_mode,
                                             "expected": wanted, "actual": result, "stderr": completed.stderr})
                if results["candidate-text"] != results["candidate-json"]:
                    failures.append({"task": key, "point": point, "error": "candidate text/JSON differ", "results": results})
                family["cases"].append({"catalog": key[0], "task": key[1], "failure_at": point, "results": results})
            print(f"{source.name}: {key[0]}/{key[1]}: {len(points) + 1} cases", flush=True)
            report.update(complete=False, failures=failures)
            args.report.write_text(json.dumps(report, indent=2) + "\n")
    report["complete"] = True
    report["failures"] = failures
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(f"consumer fixture failures: {len(failures)}", flush=True)
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
