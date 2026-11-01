import { expect, test } from "bun:test";
import { withUnreportedAgents } from "../defaults/mods/agents-personal/agent-list.js";

const known = [{key:"grok",name:"Grok",run:["grok"]},{key:"codex",name:"Codex",run:["codex"]}];
const spaces = [{id:"s1",name:"workspace",tabs:[{id:"tab1",name:"1"}]}];
const pane = {terminal:"t2",pane:"p2",tab:"tab1",focused:true};
const grok = {address:"t2",state:"live",status:null,program:{key:"grok",shell:false},labels:{}};
const codex = {address:"t1",status:"working",pane:"p1"};
test("lists a live Grok process without inventing its working state", () => {
  const result=withUnreportedAgents([codex],[pane],[grok],spaces,known);
  expect(result).toHaveLength(2);
  expect(result[0]).toEqual(codex);
  expect(result[1]).toMatchObject({address:"t2",app:"grok",status:"unknown",pane:"p2",space_name:"workspace",focused:true});
  expect(grok.status).toBeNull();
});
test("a real status report replaces the fallback without duplicate rows", () => {
  const report={address:"t2",status:"blocked",pane:"p2"};
  expect(withUnreportedAgents([report],[pane],[grok],spaces,known)).toEqual([report]);
});
test("removes Grok when the foreground returns to the shell", () => {
  const terminal={...grok,program:{key:"shell",shell:true}};
  expect(withUnreportedAgents([],[{...pane,program:{key:"grok"}}],[terminal],spaces,known)).toEqual([]);
});
test("does not list finished terminals, ordinary processes or terminals outside panes", () => {
  for(const terminal of [{...grok,state:"finished"},{...grok,program:{key:"vim",shell:false}}])
    expect(withUnreportedAgents([],[pane],[terminal],spaces,known)).toEqual([]);
  expect(withUnreportedAgents([],[],[grok],spaces,known)).toEqual([]);
});
test("keeps remote identities, tabs and pins without depending on local installation", () => {
  const remote={...grok,address:"remote/t2",labels:{"herdr.pinned":"1"}};
  const result=withUnreportedAgents([],[{...pane,terminal:"remote/t2"}],[remote],spaces,known.map(k=>({...k,installed:false})));
  expect(result[0]).toMatchObject({address:"remote/t2",tab:"tab1",tab_index:1,pinned:true});
});
