"""Deterministic task generator and exact-answer grader for Ollama Stresser.

Design
------
Every case derives its expected answer directly from information contained in
the prompt, so an assistant can reproduce the key without hidden state:

* round 0            -> remember a persistent marker (the only place the
  marker appears); expected JSON acknowledges it.
* round % 7 == 1     -> recurring recall of that marker (prompt never
  restates it).
* even round (>=2)   -> prompt sends a list of small ints; answer is the
  sorted unique positive ints plus their sum.
* odd round (>=3)    -> prompt sends an exact list of +/- deltas; answer is a
  JSON array with the running total after each step (~30-40 elements).

Grading is strict: exactly one complete fenced JSON block, no prose outside it,
and JSON bool vs int are distinguished.
"""

import json
import random


JSON_FORMAT = ("Return the answer inside exactly one Markdown JSON code block. Start with ```json "
               "on its own line, put only the JSON value inside, and end with ``` on its own line. "
               "No text outside the block.")


def _parse_fenced_json(response):
    """Return the single fenced JSON block, or None.

    Requires an opening ``` fence (optionally tagged ``json``), a complete
    closing ``` fence, and rejects any text outside the fences.
    """
    stripped = response.strip()
    if not stripped.startswith("```"):
        return None
    lines = stripped.splitlines()
    if len(lines) < 3:
        return None
    if lines[0].split(" ")[0].strip().lower() not in ("```", "```json"):
        return None
    if lines[-1].strip() != "```":
        return None
    return "\n".join(lines[1:-1])


def grade(response, expected, *, allow_bare_json=False):
    """Compare JSON values and types, optionally accepting a bare JSON reply."""
    body = _parse_fenced_json(response)
    if body is None:
        if not allow_bare_json:
            return False
        body = response.strip()
    try:
        actual = json.loads(body)
    except ValueError:
        return False
    return _typed_equal(actual, expected)


def _typed_equal(a, b):
    """Strict equality where bool vs int matters."""
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        if set(a.keys()) != set(b.keys()):
            return False
        return all(_typed_equal(a[k], b[k]) for k in a)
    if isinstance(a, list):
        if len(a) != len(b):
            return False
        return all(_typed_equal(x, y) for x, y in zip(a, b))
    return a == b


def make_case(round_number, rng, marker):
    """Return (name, prompt, expected) for one benchmark case.

    round_number is the loop index (0-based); marker is the persistent marker
    string established in round 0.
    """
    if round_number == 0:
        expected = {"acknowledged": True, "marker": marker, "type": "remember_marker"}
        prompt = (
            "Set and remember the persistent benchmark marker for this session: "
            "'%s'. Acknowledge it once by replying with exactly one JSON object: %s"
            % (marker, json.dumps(expected))
        )
        return "remember_marker", prompt, expected

    if round_number % 7 == 1:
        prompt = (
            "Recall the persistent benchmark marker you were asked to remember "
            "earlier. Reply with exactly one JSON object with the single key "
            '"marker" and its exact remembered value.'
        )
        expected = {"marker": marker}
        return "recall_marker", prompt, expected

    if round_number % 2 == 0:
        values = [rng.randint(-8, 9) for _ in range(12)]
        positive = sorted({v for v in values if v > 0})
        total = sum(positive)
        prompt = (
            "Process this list of small integers: %s.\n"
            "Reply with exactly one JSON object with keys:\n"
            '  "sorted_unique_positive": the sorted, de-duplicated list of only '
            "the strictly positive integers (values > 0);\n"
            '  "sum": the sum of those distinct positive integers.\n'
            "Both values must be derived from the input list. No extra keys, no "
            "trailing prose outside the JSON fence."
            % (values,)
        )
        expected = {"sorted_unique_positive": positive, "sum": total}
        return "uniq_positive_sum", prompt, expected

    deltas = [rng.randint(-8, 8) for _ in range(36)]
    deltas = [d if d != 0 else 1 for d in deltas]
    running = []
    total = 0
    for d in deltas:
        total += d
        running.append(total)
    prompt = (
        "A counter starts at 0. Apply these deltas in order: %s.\n"
        "Reply with exactly one JSON array containing the running total after "
        "each delta is applied (one integer per step, same length as the "
        "deltas list, in order). No extra keys, no trailing prose outside the "
        "JSON fence."
        % (deltas,)
    )
    return "running_total", prompt, running
