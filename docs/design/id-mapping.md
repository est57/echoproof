# Design: requests that refer to recorded IDs

Status: implemented (phase 1 and phase 2), unreleased

## Problem

Traffic recorded on one database refers to rows by ID: `GET /orders/9812`, `{"order_id": 9812}`. Replayed against a test database, two things go wrong.

1. **The data does not exist.** The test database never had order 9812. Both the base build and the PR build answer 404, so the diff reports the request as unchanged. The check looks green while nothing about that request was verified.
2. **The data is created under a different ID.** A recorded flow creates order 9812 and then reads `GET /orders/9812`. During replay the test database creates order 57, and the follow-up request still asks for 9812.

Case 1 is silent today and is the more dangerous one: it inflates the "unchanged" count.

## Goals

1. Never count a request as verified when it did not reproduce on the base build.
2. Make recorded flows that create and then reuse IDs replay correctly.

## Phase 1: detect requests that did not reproduce

In `check --base`, the recording is still available. A request did not reproduce when the recorded response was successful (2xx) and the base build's replay was not (4xx, 5xx, or a transport error).

- The report gets a new count next to `skipped`: `not reproduced`. Those requests are excluded from `unchanged`, listed per endpoint with their recorded and replayed status, and shown in the agent report in the same way as unverified writes ("PASS for the N reproduced requests only; M requests did not reproduce on the base build").
- A diff between base and PR on such a request is still reported, so a PR that turns a 404 into a 500 is not hidden.
- Without `--base`, the recording is the reference and a 2xx-to-404 change already shows as a breaking status change, so nothing changes there.

This needs no configuration and costs no extra requests.

## Phase 2: carry created IDs forward

During a sequential replay (concurrency 1), after each request Echoproof compares the recorded response with the replayed one. Where an identity field (the same rule as array matching: `id`, `uuid`, `code`, `*_id`, `*_no`, `*_num`, `*_code`) holds a different value, it records `recorded value -> replayed value` for that pass.

Before sending each later request, recorded values found in the path segments, the query string and identity fields of the JSON body are replaced with their replayed values. Only exact matches of whole path segments and whole field values are replaced, and only values of at least two characters (numbers above 9 or strings), to avoid rewriting unrelated small numbers.

Each pass (base, noise runs, PR) builds its own map, because each pass starts from a reset database. The replayed request is stored as sent, so `echoproof_exchange` shows the rewritten request.

A flag `--no-id-mapping` (and `[replay] map_ids = false`) turns it off.

## Out of scope

- Mapping IDs between a production recording and pre-existing test data (case 1). That needs either seeding the test database from the same snapshot as production or a user-provided mapping; phase 1 makes the gap visible instead of hiding it.
- Concurrent replays: mapping needs request order, so it applies only with `concurrency = 1`, the default for write traffic.

## Validation plan

1. Phase 1: replay the recorded POS traffic against a test database that lacks one recorded order; the report must show those requests as not reproduced, not unchanged.
2. Phase 2: record a flow on one database, replay it on a database whose ID sequence starts elsewhere; follow-up reads must succeed on base and PR, and a planted bug in the follow-up must still be caught.
