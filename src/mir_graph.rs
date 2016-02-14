use std::collections::HashMap;

use petgraph::Graph;
use petgraph::algo::*;

use rustc::mir::repr::*;

fn mk_mir_graph(mir: &Mir, start: BasicBlock, blocks: &Vec<BasicBlock>) -> Graph<BasicBlock, ()> {
    let mut g = Graph::new();
    let nodes = blocks.iter().map(|&bb| (bb.index(), g.add_node(bb))).collect::<HashMap<_, _>>();
    for &bb in blocks {
        if let Some(ref term) = mir.basic_block_data(bb).terminator {
            for succ in term.successors().iter() {
                if succ.index() != start.index() && blocks.contains(succ) {
                    g.add_edge(nodes[&bb.index()], nodes[&succ.index()], ());
                }
            }
        }
    }
    g
}

pub fn mir_sccs(mir: &Mir, start: BasicBlock, blocks: &Vec<BasicBlock>) -> Vec<Vec<BasicBlock>> {
    let g = mk_mir_graph(mir, start, blocks);
    scc(&g).iter().map(|scc| {
        scc.iter().map(|&n| *g.node_weight(n).unwrap()).collect()
    }).collect()
}
