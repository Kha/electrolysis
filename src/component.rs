use std::collections::HashMap;

use rustc::mir::repr::*;

use super::Transpiler;
use ::mir_graph::mir_sccs;

// A loop or the full function body
#[derive(Default, Debug)]
pub struct Component<'a, 'tcx: 'a> {
    pub prelude: Vec<String>,
    pub header: Option<BasicBlock>,
    pub blocks: Vec<BasicBlock>,
    pub loops: Vec<Vec<BasicBlock>>,
    pub exits: Vec<BasicBlock>,
    pub nonlocal_defs: Vec<String>,
    pub nonlocal_uses: Vec<String>,
    pub refs: HashMap<usize, &'a Lvalue<'tcx>>
}

impl<'a, 'tcx> Component<'a, 'tcx> {
    pub fn new(trans: &mut Transpiler<'a, 'tcx>, start: BasicBlock, blocks: Vec<BasicBlock>, is_loop: bool) -> Result<Component<'a, 'tcx>, String> {
        let loops = mir_sccs(trans.mir(), start, &blocks);
        let loops = loops.into_iter().filter(|l| l.len() > 1).collect::<Vec<_>>();
        let mut comp = Component {
            header: if is_loop { Some(start) } else { None },
            blocks: blocks, loops: loops,
            .. Default::default()
        };
        try!(comp.find_nonlocals(trans));
        Ok(comp)
    }

    fn find_nonlocals(&mut self, trans: &mut Transpiler<'a, 'tcx>) -> Result<(), String> {
        fn operand<'a, 'tcx>(op: &'a Operand<'tcx>, uses: &mut Vec<&'a Lvalue<'tcx>>) {
            match *op {
                Operand::Consume(ref lv) => uses.push(lv),
                Operand::Constant(_) => ()
            }
        }

        fn rvalue<'a, 'tcx>(rv: &'a Rvalue<'tcx>, uses: &mut Vec<&'a Lvalue<'tcx>>) -> Result<(), String> {
            match *rv {
                Rvalue::Use(ref op) => operand(op, uses),
                Rvalue::UnaryOp(_, ref op) => operand(op, uses),
                Rvalue::BinaryOp(_, ref o1, ref o2) => {
                    operand(o1, uses);
                    operand(o2, uses);
                }
                Rvalue::Ref(_, _, ref lv) => uses.push(lv),
                Rvalue::Aggregate(_, ref ops) => {
                    for op in ops {
                        operand(op, uses);
                    }
                }
                _ => throw!("unimplemented: find_nonlocals rvalue {:?}", rv),
            }
            Ok(())
        }

        let mut defs = Vec::new();
        let mut uses = Vec::new();
        let mut drops = Vec::new();

        for &bb in &self.blocks {
            for stmt in &trans.mir().basic_block_data(bb).statements {
                match stmt.kind {
                    StatementKind::Assign(ref lv, Rvalue::Ref(_, BorrowKind::Mut, ref ldest)) => {
                        defs.push(lv);
                        defs.push(ldest);
                    }
                    StatementKind::Assign(ref lv, ref rv) => {
                        defs.push(lv);
                        try!(rvalue(rv, &mut uses));
                    }
                    StatementKind::Drop(DropKind::Deep, ref lv) => drops.push(lv),
                    _ => throw!("unimplemented: find_nonlocals statement {:?}", stmt),
                }
            }
            if let Some(ref term) = trans.mir().basic_block_data(bb).terminator {
                if let &Terminator::Call { ref func, ref args, .. } = term {
                    operand(func, &mut uses);
                    for arg in args {
                        operand(arg, &mut uses);
                    }
                    defs.extend(try!(trans.call_return_dests(term)));
                }
            }
        }

        let ret = Lvalue::ReturnPointer;
        self.nonlocal_defs = trans.locals().iter().filter(|lv| defs.contains(lv) && !drops.contains(lv)).map(|lv| trans.lvalue_name(lv).unwrap()).collect();
        self.nonlocal_uses = trans.locals().iter().filter(|lv| **lv != ret && uses.contains(lv) && !drops.contains(lv)).map(|lv| trans.lvalue_name(lv).unwrap()).collect();
        Ok(())
    }
}
