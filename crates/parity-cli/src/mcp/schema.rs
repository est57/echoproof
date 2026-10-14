use serde_json::{Value, json};

const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Parity verifies that a code change does not alter the behavior of an HTTP service \
on real recorded traffic. After changing service code, start the changed build and call parity_check with the \
baseline snapshot and the build's URL. Describe what you meant to change in `intent` (and list the endpoints in \
`expect`). A REGRESSION verdict means responses changed in ways your intent does not explain: inspect cases with \
parity_exchange, fix the code, and check again. Never edit snapshot files, or widen the intent beyond the task you \
were given, to make a check pass. Write requests are not replayed unless you allow them in `allow_writes`, \
and then only against a disposable test environment; a report that says writes were not replayed has not \
verified them. When the service reads a database, pass `base` (a running build of the code before your change). \
Targets must be on localhost unless the project allows other hosts, and file paths must be inside the project.";

pub fn initialize(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = SUPPORTED_VERSIONS
        .iter()
        .find(|v| **v == requested)
        .unwrap_or(&SUPPORTED_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "parity", "title": "Parity", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

pub fn tools() -> Value {
    let expect = json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "Endpoints this change is meant to alter, e.g. [\"GET /products/*\", \"POST /orders\"]. \
                        `*` matches one path segment, `**` any number. Changes elsewhere are reported as regressions."
    });
    let intent = json!({
        "type": "string",
        "description": "What this change is meant to do: the task you were given and the behavior you meant to \
                        change. When set, every behavior change is judged against it by Claude (needs \
                        ANTHROPIC_API_KEY on the server), which catches unintended changes inside endpoints you \
                        meant to touch."
    });
    let code_diff_file = json!({
        "type": "string",
        "description": "Optional path (inside the project) to a file with the code diff, given to the judge."
    });
    let max_examples = json!({ "type": "integer", "minimum": 0, "description": "Example requests per change (default 3)." });
    json!([
        {
            "name": "parity_check",
            "title": "Check behavior against recorded traffic",
            "description": "Replay recorded real traffic against a running build of the changed service and report \
                            which responses changed. Use after changing service code, before declaring the work done.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "snapshot": { "type": "string", "description": "Baseline snapshot (.jsonl), relative to the project root." },
                    "target": { "type": "string", "description": "URL of the running changed build, e.g. http://127.0.0.1:3000." },
                    "expect": expect,
                    "concurrency": { "type": "integer", "minimum": 1, "description": "Parallel requests (default 1)." },
                    "save": { "type": "string", "description": "Optional path (inside the project) to save the candidate snapshot." },
                    "intent": intent,
                    "code_diff_file": code_diff_file,
                    "base": {
                        "type": "string",
                        "description": "URL of a running build of the code before your change. Both builds are then \
                                        replayed on the same state and compared with each other, and fields that \
                                        differ between two base runs are ignored as noise. Use it when the service \
                                        reads a database."
                    },
                    "allow_writes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Endpoints whose write requests may be replayed, e.g. [\"POST /orders\"]. Only \
                                        against builds and databases you started yourself for testing."
                    },
                    "noise_runs": { "type": "integer", "minimum": 0, "description": "Extra base replays for learning noise (default 1)." },
                    "max_examples": max_examples,
                },
                "required": ["snapshot", "target"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true },
        },
        {
            "name": "parity_diff",
            "title": "Compare two snapshots",
            "description": "Compare a baseline snapshot with a candidate snapshot that was already replayed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "baseline": { "type": "string" },
                    "candidate": { "type": "string" },
                    "expect": expect,
                    "intent": intent,
                    "code_diff_file": code_diff_file,
                    "max_examples": max_examples,
                },
                "required": ["baseline", "candidate"],
            },
            "annotations": { "readOnlyHint": true },
        },
        {
            "name": "parity_exchange",
            "title": "Inspect one replayed request",
            "description": "Full request, reference response, new response and field-level changes for one exchange \
                            from a previous parity_check or parity_diff run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "run_id from parity_check or parity_diff." },
                    "id": { "type": "string", "description": "Exchange id shown in the report examples." },
                },
                "required": ["run_id", "id"],
            },
            "annotations": { "readOnlyHint": true },
        },
    ])
}
