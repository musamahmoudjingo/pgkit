use syn::{Block, Expr, ExprAsync, Stmt};

/// Find the block of the future the desugared method builds: the async
/// block inside `Box::pin(async move { ... })` (or close variations of that
/// expression shape).
pub(super) fn find_async_block(block: &mut Block) -> Option<&mut Block> {
    block.stmts.iter_mut().find_map(|stmt| match stmt {
        Stmt::Expr(expr, _) => find_in_expr(expr),
        _ => None,
    })
}

fn find_in_expr(expr: &mut Expr) -> Option<&mut Block> {
    match expr {
        Expr::Async(ExprAsync { block, .. }) => Some(block),
        Expr::Call(call) => call.args.iter_mut().find_map(find_in_expr),
        Expr::MethodCall(call) => {
            find_in_expr(&mut call.receiver).or_else(|| call.args.iter_mut().find_map(find_in_expr))
        }
        Expr::Paren(paren) => find_in_expr(&mut paren.expr),
        Expr::Group(group) => find_in_expr(&mut group.expr),
        Expr::Block(inner) => find_async_block(&mut inner.block),
        _ => None,
    }
}
