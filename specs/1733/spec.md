# Issue 1733 Spec

Status: Implemented

Issue: `#1733`  
Milestone: `#22`  
Parent: `#1621`

## Problem Statement

Docs now link to future true-RL Epic `#1657` and Milestone `#24`, but there is
no repository-local roadmap skeleton that defines phased/staged delivery and
exit gates. Without this artifact, operators cannot quickly interpret what
"future true RL" concretely means.

## Scope

In scope:

- add a dedicated planning doc describing true-RL phases/stages
- map each stage to existing milestone `#24` stories/tasks
- define stage entry/exit evidence expectations
- link skeleton doc from naming/training docs

Out of scope:

- implementation of RL runtime or optimizer code
- changes to prompt-optimization runtime behavior

## Acceptance Criteria

AC-1 (phase/stage skeleton):
Given repository docs,
when roadmap skeleton is read,
then true-RL phases and stage goals are explicit and ordered.

AC-2 (issue mapping):
Given each phase,
when stage details are inspected,
then linked milestone `#24` issue IDs are provided.

AC-3 (boundary clarity):
Given operator-facing training docs,
when users navigate roadmap links,
then current prompt-optimization and future true-RL scopes are clearly separated.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `docs/planning/true-rl-roadmap-skeleton.md`, when read, then ordered stages with objectives and exit gates are present. |
| C-02 | AC-2 | Conformance | Given stage sections, when checking references, then each stage contains milestone `#24` issue mappings. |
| C-03 | AC-3 | Functional | Given training docs, when following links, then roadmap skeleton + Epic `#1657` + Milestone `#24` are reachable. |
| C-04 | AC-3 | Regression | Given docs link checks, when run, then suite passes after adding new planning links. |

## Success Metrics

- Future true-RL scope is documented with explicit staged phases
- Every phase maps to concrete GitHub issues in milestone `#24`
- Operator docs retain clear current-vs-future boundary language
