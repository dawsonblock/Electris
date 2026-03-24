# Core API Surface

This is the intended minimum HTTP surface for the corrected Electris build.

## Required endpoints

- `POST /message`
- `GET /stream`
- `GET /health/live`
- `GET /health/ready`

## Behavioral rules

- gateway starts independently of provider readiness
- readiness is reported by `/health/ready`
- request IDs should be returned for traceability
- runtime output should be streamed through the event model
