#!/usr/bin/env python3
import json
import sys


def main() -> int:
    command = sys.argv[1] if len(sys.argv) > 1 else ""

    if command == "start-session":
        print(json.dumps({"status": "ok"}))
        return 0

    if command == "shutdown-session":
        print(json.dumps({"status": "ok"}))
        return 0

    if command != "execute-action":
        print("unsupported command", file=sys.stderr)
        return 2

    payload = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    operation = str(payload.get("operation", "")).strip().lower()

    if operation == "snapshot":
        print(
            json.dumps(
                {
                    "status_code": 200,
                    "response_body": {
                        "status": "ok",
                        "operation": "snapshot",
                        "snapshot_id": "snapshot-live",
                        "elements": [{"id": "e1", "role": "button", "name": "Run"}],
                    },
                }
            )
        )
        return 0

    if operation == "action":
        print(
            json.dumps(
                {
                    "status_code": 200,
                    "response_body": {
                        "status": "ok",
                        "operation": "action",
                        "action": payload.get("action", ""),
                        "selector": payload.get("selector", ""),
                        "repeat_count": payload.get("action_repeat_count", 1),
                        "text": payload.get("text", ""),
                        "timeout_ms": payload.get("timeout_ms", 0),
                    },
                }
            )
        )
        return 0

    print(
        json.dumps(
            {
                "status_code": 400,
                "error_code": "browser_automation_invalid_operation",
                "response_body": {"status": "rejected", "reason": "invalid_operation"},
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
