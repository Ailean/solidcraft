# Narrow-card Pause fix - independently source-reviewed, not executed

C13 stopped on the original real UI assertion: at760x700, Pause y380..408 exceeded the card's visible y327..381. The observed native test exited101; overall1case passed,1failed,51did not run. Keep that failure and the original test unchanged.

This one-file source proposal keeps the real action row outside and before the scrollable status, and limits status to remaining inner height. Ready/disabled logic, labels, actual target rectangles and command dispatch stay unchanged. It does not move Nathan's selection toolbar, panels, Results, Studio or gizmos, hide a failed test, raise a minimum-window threshold or crop target rectangles to fake containment. In a card with no remaining height, status is omitted; actions take priority.

Base render_workspace_interactive.rs SHA2562b385ccba4e94aaff6318f70a3669cd8205d786edad502e01e3c1e03f2e43579; candidatefdc1c1cfa2a97671bcbb94d3dae7d3502eb61e9b9d676011c96baa7bddc37970. Engineer and independent Designer source review found no blocker for this narrow change. The213-test source and all assertions remain intact. No formatting, compilation, test execution or new native button coordinates exist for the fix. A new exact-source serial validation remains required; C13 is never relabelled as passed.
