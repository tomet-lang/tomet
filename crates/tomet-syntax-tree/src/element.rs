//! Extension traits, property helpers, and constructors for [`tomet_ast::Element`], [`tomet_ast::Declaration`] and [`tomet_ast::Data`].

use tomet_ast::{
    Block, Connect, Data, Declaration, Element, Group, Id, List, ListItem, Name, Placement, Tuple,
    Value,
};
use tomet_span::Span;

/// Extension trait providing accessors, attribute manipulations, and inspections on [`Element`].
pub trait ElementExt {
    /// Consumes `self` and sets its `span`.
    fn with_span(self, span: Span) -> Self;

    /// Consumes `self` and sets its `placement`.
    fn with_placement(self, placement: Placement) -> Self;

    /// Consumes `self` and sets its `args`.
    fn with_args(self, args: Tuple) -> Self;

    /// Consumes `self` and sets its `content`.
    fn with_content(self, content: Vec<Block>) -> Self;

    /// Consumes `self` and sets its `value`.
    fn with_value(self, value: Group) -> Self;

    /// Consumes `self` and sets its `id`.
    fn with_id(self, id: Id) -> Self;

    /// Returns just the local half of the element's name, without its
    /// namespace. Prefer [`Element::name`] or [`Name::is_bare_named`] when
    /// the namespace matters.
    fn local_name(&self) -> &str;
}

impl ElementExt for Element {
    fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    fn with_placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    fn with_args(mut self, args: Tuple) -> Self {
        self.data.args = Some(args);
        self
    }

    fn with_content(mut self, content: Vec<Block>) -> Self {
        self.content = Some(content);
        self
    }

    fn with_value(mut self, value: Group) -> Self {
        self.data.value = Some(value);
        self
    }

    fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    fn local_name(&self) -> &str {
        &self.name.name
    }
}

/// Extension trait providing property access on [`Data`]: what every node that
/// carries `(args)` and a `{value}` group shares, so an element, a declaration,
/// a section and a connect read and write their properties the same way.
pub trait DataExt {
    /// Merges the `args` and the `key: value` pairs of the
    /// value group into one attrs view -- the value group's keys win on
    /// conflict.
    fn attrs_view(&self) -> Option<Tuple>;

    /// Mutable writable attrs slot on `el.args`.
    ///
    /// Unlike [`DataExt::attrs_view`] this does not fall back to the
    /// value group: a group is a list of [`tomet_ast::Entry`] items, not a
    /// `Tuple`, so there is no single `&mut Tuple` to hand out for it. Use
    /// [`DataExt::set_prop`] or `Group`'s own pair accessors to
    /// write into a group.
    fn attrs_mut(&mut self) -> Option<&mut Tuple>;

    /// Returns a reference to the property value for `key`, checking `{value}` first, then `(args)`.
    fn get_attr<'a>(&'a self, key: &str) -> Option<&'a Value>;

    /// Returns a mutable reference to the property value for `key`, checking `{value}` first, then `(args)`.
    fn get_attr_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut Value>;

    /// Returns whether the element contains `key` in either `{value}` or `(args)`.
    fn has_prop_key(&self, key: &str) -> bool;

    /// Sets or updates a property on `el`. If the key exists in `{value}` or `(args)`,
    /// updates it in place; otherwise appends to `{value}` or `(args)`.
    fn set_prop(&mut self, key: &str, new_val: Value);

    /// Renames all occurrences of `old_key` to `new_key` in `(args)` and `{value}`.
    fn rename_prop_key(&mut self, old_key: &str, new_key: &str) -> bool;

    /// Replaces all values associated with `target_key` with `new_val` in `(args)` and `{value}`.
    fn replace_prop_value(&mut self, target_key: &str, new_val: Value) -> bool;

    /// Removes a property from `el` and returns its previous [`Value`], if present.
    fn remove_prop(&mut self, key: &str) -> Option<Value>;

    /// Transforms the property value for `key` using a closure `f`, if present.
    fn transform_prop<F>(&mut self, key: &str, f: F) -> bool
    where
        F: FnOnce(Value) -> Value;
}

impl DataExt for Data {
    fn attrs_view(&self) -> Option<Tuple> {
        let value_tuple = self.value.as_ref().map(Group::as_data);
        match (&self.args, value_tuple) {
            (Some(args), Some(value_entries)) => {
                let mut merged = args.clone();
                for (key, value) in value_entries.into_named() {
                    match merged.get_mut(&key) {
                        Some(existing) => *existing = value,
                        None => merged.push_named(key, value),
                    }
                }
                Some(merged)
            }
            (Some(args), None) => Some(args.clone()),
            (None, Some(value)) => Some(value),
            (None, None) => None,
        }
    }

    fn attrs_mut(&mut self) -> Option<&mut Tuple> {
        self.args.as_mut()
    }

    fn get_attr<'a>(&'a self, key: &str) -> Option<&'a Value> {
        if let Some(value) = &self.value
            && let Some(found) = value.get(key)
        {
            return Some(found);
        }
        if let Some(args) = &self.args
            && let Some(val) = args.get(key)
        {
            return Some(val);
        }
        None
    }

    fn get_attr_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut Value> {
        // Checked separately from the mutable borrow below so `value` and
        // `args` are never borrowed mutably at the same time.
        if self.value.as_ref().is_some_and(|v| v.get(key).is_some()) {
            return self.value.as_mut().and_then(|v| v.get_mut(key));
        }
        if let Some(args) = &mut self.args
            && let Some(val) = args.get_mut(key)
        {
            return Some(val);
        }
        None
    }

    fn has_prop_key(&self, key: &str) -> bool {
        self.get_attr(key).is_some()
    }

    fn set_prop(&mut self, key: &str, new_val: Value) {
        if let Some(value) = &mut self.value
            && let Some(slot) = value.get_mut(key)
        {
            *slot = new_val;
            return;
        }
        if let Some(args) = &mut self.args
            && let Some(val) = args.get_mut(key)
        {
            *val = new_val;
            return;
        }
        if let Some(value @ Group(_)) = &mut self.value {
            value.push_pair(key, new_val);
        } else if let Some(args) = &mut self.args {
            args.push_named(key, new_val);
        } else {
            self.args = Some(Tuple::from_named([(key.to_string(), new_val)]));
        }
    }

    fn rename_prop_key(&mut self, old_key: &str, new_key: &str) -> bool {
        let mut changed = false;
        if let Some(args) = &mut self.args {
            for entry in args.entries_mut() {
                if entry.name() == Some(old_key) {
                    entry.set_name(new_key);
                    changed = true;
                }
            }
        }
        if let Some(value) = &mut self.value {
            for (k, _) in value.pairs_mut() {
                if k == old_key {
                    *k = new_key.to_string();
                    changed = true;
                }
            }
        }
        changed
    }

    fn replace_prop_value(&mut self, target_key: &str, new_val: Value) -> bool {
        let mut changed = false;
        if let Some(args) = &mut self.args {
            for (k, val) in args.named_mut() {
                if k == target_key {
                    *val = new_val.clone();
                    changed = true;
                }
            }
        }
        if let Some(value) = &mut self.value {
            for (k, val) in value.pairs_mut() {
                if k == target_key {
                    *val = new_val.clone();
                    changed = true;
                }
            }
        }
        changed
    }

    fn remove_prop(&mut self, key: &str) -> Option<Value> {
        if let Some(value) = &mut self.value
            && let Some(removed) = value.remove(key)
        {
            return Some(removed);
        }
        if let Some(args) = &mut self.args
            && let Some(removed) = args.remove(key)
        {
            return Some(removed);
        }
        None
    }

    fn transform_prop<F>(&mut self, key: &str, f: F) -> bool
    where
        F: FnOnce(Value) -> Value,
    {
        if let Some(val_mut) = self.get_attr_mut(key) {
            let old = std::mem::replace(val_mut, Value::Null);
            *val_mut = f(old);
            true
        } else {
            false
        }
    }
}

/// Creates a new [`Element`] with given `name` and default empty fields.
///
/// The placement is [`Placement::Inline`]; a caller building a block uses
/// [`ElementExt::with_placement`], and the parser sets it from position.
pub fn element_new(name: Name) -> Element {
    Element {
        name,
        placement: Placement::Inline,
        data: Data::default(),
        content: None,
        id: None,
        connects: Vec::new(),
        span: Span::default(),
    }
}

/// Creates a new [`Declaration`] named with the bare name `name` and every
/// group empty.
pub fn declaration_new(name: impl Into<String>) -> Declaration {
    Declaration {
        name: Name::bare(name),
        data: Data::default(),
        content: None,
        id: None,
        span: Span::default(),
    }
}

/// A list constructor (ordered if `ordered`, i.e. `-.`, else `-`).
pub fn list(ordered: bool, items: Vec<ListItem>, span: Span) -> List {
    List {
        ordered,
        items,
        id: None,
        connects: Vec::new(),
        span,
    }
}

/// A list item constructor. `name` is `None` for a plain `- content` item, or
/// the element's name for the `-@name(...)` combine form -- which takes the
/// same groups a standalone `@name(...)` would.
///
/// `id` is a real parameter, not hardcoded to `None` -- list items are a
/// first-class consumer of `#(id)` like everything else `parse_groups`
/// reads for, and silently dropping it here would lose it, as dropping
/// `connects` would (a full-form list item's parsed connects would be
/// read and then thrown away at this exact spot).
#[allow(clippy::too_many_arguments)]
pub fn list_item(
    name: Option<Name>,
    content: Vec<Block>,
    args: Option<Tuple>,
    attrs: Option<Tuple>,
    id: Option<Id>,
    connects: Vec<Connect>,
    sublist: Option<List>,
    span: Span,
) -> ListItem {
    ListItem {
        name,
        data: Data {
            args,
            value: attrs.map(Group::from_map),
        },
        content,
        id,
        connects,
        sublist,
        span,
    }
}
