# M186 - Credential Store Key Rotation Command

## Context
Encrypted credential-store flows exist, but operators lack a first-class command to rotate encryption keys while preserving provider and integration secrets.

## Scope
- Add `/auth rotate-key` command parser + execution path.
- Re-encrypt credential-store payloads with new key safely.
- Update command catalog/help and auth usage strings.
- Add conformance tests for parse/help/success/error behavior.

## Linked Issues
- Epic: #3030
- Story: #3031
- Task: #3032
