# Roadmap Execution Index

This guide is the repository-local execution index for `tasks/todo.md` delivery.

It maps each roadmap item to:
- milestone
- epic
- story
- task

Snapshot date: 2026-02-15 (UTC)

## Coverage Summary

- Milestones:
  - [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18)
  - [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10)
  - [#11 Gap List P1 Core Tools](https://github.com/njfio/Tau/milestone/11)
  - [#12 Gap List P2 Memory Persistence](https://github.com/njfio/Tau/milestone/12)
  - [#13 Gap List P3 Sandbox](https://github.com/njfio/Tau/milestone/13)
  - [#14 Gap List P4 Innovation](https://github.com/njfio/Tau/milestone/14)
  - [#15 Gap List P5 Operations](https://github.com/njfio/Tau/milestone/15)
  - [#16 Gap List P6 Deployment API](https://github.com/njfio/Tau/milestone/16)
  - [#17 Gap List P7 Documentation](https://github.com/njfio/Tau/milestone/17)
  - [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19)
  - [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20)
- Open epics in this index: 11
- Open stories in this index: 45
- Open tasks in this index: 45

## Naming Alignment Overlays (M22+)

These overlays separate current prompt-optimization taxonomy from future
true-RL planning:

- [#6 Agent Lightning Prompt Optimization Port (Tau)](https://github.com/njfio/Tau/milestone/6)
  - historical "Agent Lightning RL Port" lane renamed for current-scope accuracy
- [#22 Gap Closure Wave 2026-04: Prompt Optimization Naming Alignment](https://github.com/njfio/Tau/milestone/22)
  - rename, compatibility alias, and terminology scan gate
- [#24 True RL Wave 2026-Q3: Policy Learning in Production](https://github.com/njfio/Tau/milestone/24)
  - dedicated future true-RL delivery wave

Future true-RL staged roadmap:
- [`docs/planning/true-rl-roadmap-skeleton.md`](../planning/true-rl-roadmap-skeleton.md)

## Execution Order

1. [Phase 0 Scaffold Cleanup](https://github.com/njfio/Tau/issues/1484)
2. [P0 Security](https://github.com/njfio/Tau/issues/1422)
3. [P1 Core Tools](https://github.com/njfio/Tau/issues/1423)
4. [P2 Memory/Persistence](https://github.com/njfio/Tau/issues/1424)
5. [P8 KAMN Integration](https://github.com/njfio/Tau/issues/1495)
6. [P3 Sandbox](https://github.com/njfio/Tau/issues/1425)
7. [P4 Innovation](https://github.com/njfio/Tau/issues/1426)
8. [P5 Operations](https://github.com/njfio/Tau/issues/1427)
9. [P6 Deployment/API](https://github.com/njfio/Tau/issues/1428)
10. [P7 Documentation + 9.2 Architecture Docs](https://github.com/njfio/Tau/issues/1429)
11. [Cleanup Backlog](https://github.com/njfio/Tau/issues/1508)

## Epic Index

| Order | Milestone | Epic |
| --- | --- | --- |
| 1 | [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18) | [#1484 Epic: Phase 0 Scaffold-to-Live Completion (0.1-0.5)](https://github.com/njfio/Tau/issues/1484) |
| 2 | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1422 Epic: P0 Security Baseline (1.1, 1.2, 1.3, 1.4, 1.6, 1.7)](https://github.com/njfio/Tau/issues/1422) |
| 3 | [#11 Gap List P1 Core Tools](https://github.com/njfio/Tau/milestone/11) | [#1423 Epic: P1 Core Tools Expansion (2.1, 2.3, 2.5, 2.6)](https://github.com/njfio/Tau/issues/1423) |
| 4 | [#12 Gap List P2 Memory Persistence](https://github.com/njfio/Tau/milestone/12) | [#1424 Epic: P2 Memory and Persistence Upgrade (3.1-3.4)](https://github.com/njfio/Tau/issues/1424) |
| 5 | [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19) | [#1495 Epic: P8 KAMN Integration and Trusted Coordination (8.1-8.5)](https://github.com/njfio/Tau/issues/1495) |
| 6 | [#13 Gap List P3 Sandbox](https://github.com/njfio/Tau/milestone/13) | [#1425 Epic: P3 Sandbox Hardening (1.5, 1.8)](https://github.com/njfio/Tau/issues/1425) |
| 7 | [#14 Gap List P4 Innovation](https://github.com/njfio/Tau/milestone/14) | [#1426 Epic: P4 Innovation Layer (2.2, 2.4)](https://github.com/njfio/Tau/issues/1426) |
| 8 | [#15 Gap List P5 Operations](https://github.com/njfio/Tau/milestone/15) | [#1427 Epic: P5 Operational Autonomy (4.1-4.4)](https://github.com/njfio/Tau/issues/1427) |
| 9 | [#16 Gap List P6 Deployment API](https://github.com/njfio/Tau/milestone/16) | [#1428 Epic: P6 Deployment and API Parity (5.1-5.3)](https://github.com/njfio/Tau/issues/1428) |
| 10 | [#17 Gap List P7 Documentation](https://github.com/njfio/Tau/milestone/17) | [#1429 Epic: P7 Documentation Density Uplift (6.1)](https://github.com/njfio/Tau/issues/1429) |
| 11 | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1508 Epic: Cleanup Backlog Execution (Code Quality and Integration Test Gaps)](https://github.com/njfio/Tau/issues/1508) |

## Story-to-Task Mapping

| Item | Milestone | Story | Task |
| --- | --- | --- | --- |
| `1.1` | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1430](https://github.com/njfio/Tau/issues/1430) | [#1431](https://github.com/njfio/Tau/issues/1431) |
| `1.2` | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1432](https://github.com/njfio/Tau/issues/1432) | [#1433](https://github.com/njfio/Tau/issues/1433) |
| `1.3` | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1434](https://github.com/njfio/Tau/issues/1434) | [#1435](https://github.com/njfio/Tau/issues/1435) |
| `1.4` | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1436](https://github.com/njfio/Tau/issues/1436) | [#1437](https://github.com/njfio/Tau/issues/1437) |
| `1.6` | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1440](https://github.com/njfio/Tau/issues/1440) | [#1441](https://github.com/njfio/Tau/issues/1441) |
| `1.7` | [#10 Gap List P0 Security](https://github.com/njfio/Tau/milestone/10) | [#1442](https://github.com/njfio/Tau/issues/1442) | [#1443](https://github.com/njfio/Tau/issues/1443) |
| `2.1` | [#11 Gap List P1 Core Tools](https://github.com/njfio/Tau/milestone/11) | [#1446](https://github.com/njfio/Tau/issues/1446) | [#1447](https://github.com/njfio/Tau/issues/1447) |
| `2.3` | [#11 Gap List P1 Core Tools](https://github.com/njfio/Tau/milestone/11) | [#1450](https://github.com/njfio/Tau/issues/1450) | [#1451](https://github.com/njfio/Tau/issues/1451) |
| `2.5` | [#11 Gap List P1 Core Tools](https://github.com/njfio/Tau/milestone/11) | [#1454](https://github.com/njfio/Tau/issues/1454) | [#1455](https://github.com/njfio/Tau/issues/1455) |
| `2.6` | [#11 Gap List P1 Core Tools](https://github.com/njfio/Tau/milestone/11) | [#1456](https://github.com/njfio/Tau/issues/1456) | [#1457](https://github.com/njfio/Tau/issues/1457) |
| `3.1` | [#12 Gap List P2 Memory Persistence](https://github.com/njfio/Tau/milestone/12) | [#1458](https://github.com/njfio/Tau/issues/1458) | [#1459](https://github.com/njfio/Tau/issues/1459) |
| `3.2` | [#12 Gap List P2 Memory Persistence](https://github.com/njfio/Tau/milestone/12) | [#1460](https://github.com/njfio/Tau/issues/1460) | [#1461](https://github.com/njfio/Tau/issues/1461) |
| `3.3` | [#12 Gap List P2 Memory Persistence](https://github.com/njfio/Tau/milestone/12) | [#1462](https://github.com/njfio/Tau/issues/1462) | [#1463](https://github.com/njfio/Tau/issues/1463) |
| `3.4` | [#12 Gap List P2 Memory Persistence](https://github.com/njfio/Tau/milestone/12) | [#1464](https://github.com/njfio/Tau/issues/1464) | [#1465](https://github.com/njfio/Tau/issues/1465) |
| `1.5` | [#13 Gap List P3 Sandbox](https://github.com/njfio/Tau/milestone/13) | [#1438](https://github.com/njfio/Tau/issues/1438) | [#1439](https://github.com/njfio/Tau/issues/1439) |
| `1.8` | [#13 Gap List P3 Sandbox](https://github.com/njfio/Tau/milestone/13) | [#1444](https://github.com/njfio/Tau/issues/1444) | [#1445](https://github.com/njfio/Tau/issues/1445) |
| `2.2` | [#14 Gap List P4 Innovation](https://github.com/njfio/Tau/milestone/14) | [#1448](https://github.com/njfio/Tau/issues/1448) | [#1449](https://github.com/njfio/Tau/issues/1449) |
| `2.4` | [#14 Gap List P4 Innovation](https://github.com/njfio/Tau/milestone/14) | [#1452](https://github.com/njfio/Tau/issues/1452) | [#1453](https://github.com/njfio/Tau/issues/1453) |
| `4.1` | [#15 Gap List P5 Operations](https://github.com/njfio/Tau/milestone/15) | [#1466](https://github.com/njfio/Tau/issues/1466) | [#1467](https://github.com/njfio/Tau/issues/1467) |
| `4.2` | [#15 Gap List P5 Operations](https://github.com/njfio/Tau/milestone/15) | [#1468](https://github.com/njfio/Tau/issues/1468) | [#1469](https://github.com/njfio/Tau/issues/1469) |
| `4.3` | [#15 Gap List P5 Operations](https://github.com/njfio/Tau/milestone/15) | [#1470](https://github.com/njfio/Tau/issues/1470) | [#1471](https://github.com/njfio/Tau/issues/1471) |
| `4.4` | [#15 Gap List P5 Operations](https://github.com/njfio/Tau/milestone/15) | [#1472](https://github.com/njfio/Tau/issues/1472) | [#1473](https://github.com/njfio/Tau/issues/1473) |
| `5.1` | [#16 Gap List P6 Deployment API](https://github.com/njfio/Tau/milestone/16) | [#1474](https://github.com/njfio/Tau/issues/1474) | [#1478](https://github.com/njfio/Tau/issues/1478) |
| `5.2` | [#16 Gap List P6 Deployment API](https://github.com/njfio/Tau/milestone/16) | [#1475](https://github.com/njfio/Tau/issues/1475) | [#1479](https://github.com/njfio/Tau/issues/1479) |
| `5.3` | [#16 Gap List P6 Deployment API](https://github.com/njfio/Tau/milestone/16) | [#1476](https://github.com/njfio/Tau/issues/1476) | [#1480](https://github.com/njfio/Tau/issues/1480) |
| `6.1` | [#17 Gap List P7 Documentation](https://github.com/njfio/Tau/milestone/17) | [#1477](https://github.com/njfio/Tau/issues/1477) | [#1481](https://github.com/njfio/Tau/issues/1481) |
| `9.2` | [#17 Gap List P7 Documentation](https://github.com/njfio/Tau/milestone/17) | [#1506](https://github.com/njfio/Tau/issues/1506) | [#1507](https://github.com/njfio/Tau/issues/1507) |
| `0.1` | [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18) | [#1485](https://github.com/njfio/Tau/issues/1485) | [#1486](https://github.com/njfio/Tau/issues/1486) |
| `0.2` | [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18) | [#1487](https://github.com/njfio/Tau/issues/1487) | [#1488](https://github.com/njfio/Tau/issues/1488) |
| `0.3` | [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18) | [#1489](https://github.com/njfio/Tau/issues/1489) | [#1490](https://github.com/njfio/Tau/issues/1490) |
| `0.4` | [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18) | [#1491](https://github.com/njfio/Tau/issues/1491) | [#1492](https://github.com/njfio/Tau/issues/1492) |
| `0.5` | [#18 Gap List P-Now Scaffold Cleanup](https://github.com/njfio/Tau/milestone/18) | [#1493](https://github.com/njfio/Tau/issues/1493) | [#1494](https://github.com/njfio/Tau/issues/1494) |
| `8.1` | [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19) | [#1496](https://github.com/njfio/Tau/issues/1496) | [#1497](https://github.com/njfio/Tau/issues/1497) |
| `8.2` | [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19) | [#1498](https://github.com/njfio/Tau/issues/1498) | [#1499](https://github.com/njfio/Tau/issues/1499) |
| `8.3` | [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19) | [#1500](https://github.com/njfio/Tau/issues/1500) | [#1501](https://github.com/njfio/Tau/issues/1501) |
| `8.4` | [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19) | [#1502](https://github.com/njfio/Tau/issues/1502) | [#1503](https://github.com/njfio/Tau/issues/1503) |
| `8.5` | [#19 Gap List P8 KAMN Integration](https://github.com/njfio/Tau/milestone/19) | [#1504](https://github.com/njfio/Tau/issues/1504) | [#1505](https://github.com/njfio/Tau/issues/1505) |
| `Cleanup 1` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1509](https://github.com/njfio/Tau/issues/1509) | [#1510](https://github.com/njfio/Tau/issues/1510) |
| `Cleanup 2` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1511](https://github.com/njfio/Tau/issues/1511) | [#1512](https://github.com/njfio/Tau/issues/1512) |
| `Cleanup 3` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1513](https://github.com/njfio/Tau/issues/1513) | [#1514](https://github.com/njfio/Tau/issues/1514) |
| `Cleanup 4` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1515](https://github.com/njfio/Tau/issues/1515) | [#1516](https://github.com/njfio/Tau/issues/1516) |
| `Cleanup 5` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1517](https://github.com/njfio/Tau/issues/1517) | [#1518](https://github.com/njfio/Tau/issues/1518) |
| `Cleanup 6` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1519](https://github.com/njfio/Tau/issues/1519) | [#1520](https://github.com/njfio/Tau/issues/1520) |
| `Cleanup 7` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1521](https://github.com/njfio/Tau/issues/1521) | [#1522](https://github.com/njfio/Tau/issues/1522) |
| `Cleanup 8` | [#20 Gap List Cleanup Backlog](https://github.com/njfio/Tau/milestone/20) | [#1523](https://github.com/njfio/Tau/issues/1523) | [#1524](https://github.com/njfio/Tau/issues/1524) |

## Governance

- All delivery remains issue-first.
- Each implementation PR should link its task issue (`Closes #<task>`).
- Story issues should be closed after all child task acceptance criteria and test matrix evidence are complete.
