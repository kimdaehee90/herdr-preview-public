// A process can be a known AI without speaking Herdr's status protocol.
// Only the live foreground program registry identifies it; never guess a state
// from a title/spinner, and never write a fabricated status back to the core.
export function withUnreportedAgents(reported, panes, terminals, spaces, known) {
  const result = [...reported];
  const listed = new Set(reported.map(a => a.address));
  const live = new Map(terminals.filter(t => t.state === "live").map(t => [t.address, t]));
  const programs = new Map(known.map(p => [p.key, p]));
  for (const pane of panes) {
    if (!pane.terminal || listed.has(pane.terminal)) continue;
    const terminal = live.get(pane.terminal);
    const current = terminal?.program;
    const program = programs.get(current?.key);
    if (!program || current.shell) continue;
    const space = spaces.find(s => s.tabs.some(t => t.id === pane.tab));
    if (!space) continue;
    const index = space.tabs.findIndex(t => t.id === pane.tab);
    result.push({
      address: pane.terminal, pane: pane.pane, space: space.id, space_name: space.name,
      tab: pane.tab, tab_index: index + 1, tab_name: space.tabs[index].name,
      status: "unknown", app: program.key, program: { ...current, ...program },
      title: terminal.title || pane.title || program.name, seq: 0,
      pinned: !!terminal.labels?.["herdr.pinned"], focused: !!pane.focused,
    });
    listed.add(pane.terminal);
  }
  return result;
}
