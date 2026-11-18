//! Deterministic AST part lookup by identifier.

use tomet_ast::File;

/// Finds the first node with unique id `id` in `doc`, including inside `::connect`
/// blocks, and maps it via `map`.
pub fn find_by_id_with<T>(
    doc: &File,
    id: &str,
    mut map: impl FnMut(tomet_tree::Node<'_>) -> Option<T>,
) -> Option<T> {
    let found = tomet_tree::walk(doc, &mut |node, _| {
        if node.id().is_some_and(|i| i.0 == id)
            && let Some(mapped) = map(node)
        {
            return tomet_tree::Visit::Break(mapped);
        }
        tomet_tree::Visit::Descend
    });
    match found {
        std::ops::ControlFlow::Break(value) => Some(value),
        std::ops::ControlFlow::Continue(()) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::find_by_id_with;

    #[test]
    fn finds_nodes_by_id_across_blocks_args_and_holes() {
        let doc = tomet_ast_parser::parse_file(
            "=[ T @a()#(in_title) ]\n\n@b(icon: @c()#(in_args))\n\n<?(h)>[@d()#(in_hole)]\n",
        )
        .unwrap();
        for (id, expected_name) in [("in_title", "a"), ("in_args", "c"), ("in_hole", "d")] {
            let name = find_by_id_with(&doc, id, |node| {
                node.element().map(|el| el.name.to_string())
            });
            assert_eq!(name.as_deref(), Some(expected_name));
        }
        assert_eq!(find_by_id_with(&doc, "missing", |_| Some(())), None);
    }
}
