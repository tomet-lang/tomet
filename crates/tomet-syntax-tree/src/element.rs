//! Extension traits, property helpers, and constructors for [`tomet_ast::Element`], [`tomet_ast::Declaration`] and [`tomet_ast::Data`].

use tomet_ast::{
    Block, Connect, Data, Declaration, Element, Group, Id, List, ListItem, Name, Placement, Value,
};
use tomet_span::Span;

/// Extension trait providing accessors, attribute manipulations, and inspections on [`Element`].
pub trait ElementExt {
    /// Consumes `self` and sets its `span`.
    fn with_span(self, span: Span) -> Self;

    /// Consumes `self` and sets its `placement`.
    fn with_placement(self, placement: Placement) -> Self;

    /// Consumes `self` and sets its `args`.
    fn with_args(self, args: Value) -> Self;

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

    fn with_args(mut self, args: Value) -> Self {
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
    fn attrs_view(&self) -> Option<Value>;

    /// Mutable writable attrs slot on `el.args`.
    ///
    /// Unlike [`DataExt::attrs_view`] this does not fall back to the
    /// value group: a group is a list of [`tomet_ast::Entry`] items, not a
    /// `Value`, so there is no single `&mut Value` to hand out for it. Use
    /// [`DataExt::set_prop`] or `Group`'s own pair accessors to
    /// write into a group.
    fn attrs_mut(&mut self) -> Option<&mut Value>;

    /// Returns a reference to the property value for `key`, checking `{value}` map first, then `(args)` map.
    fn get_attr<'a>(&'a self, key: &str) -> Option<&'a Value>;

    /// Returns a mutable reference to the property value for `key`, checking `{value}` map first, then `(args)` map.
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
    fn attrs_view(&self) -> Option<Value> {
        let args_map = match &self.args {
            Some(Value::Map(entries)) => Some(entries.clone()),
            _ => None,
        };
        let value_map = self.value.as_ref().and_then(|v| match v.as_data() {
            Value::Map(entries) => Some(entries),
            _ => None,
        });
        match (args_map, value_map) {
            (Some(mut merged), Some(value_entries)) => {
                for (key, value) in value_entries {
                    match merged.iter_mut().find(|(k, _)| *k == key) {
                        Some(existing) => existing.1 = value,
                        None => merged.push((key, value)),
                    }
                }
                Some(Value::Map(merged))
            }
            (Some(args), None) => Some(Value::Map(args)),
            (None, Some(value)) => Some(Value::Map(value)),
            (None, None) => self.args.clone(),
        }
    }

    fn attrs_mut(&mut self) -> Option<&mut Value> {
        self.args.as_mut()
    }

    fn get_attr<'a>(&'a self, key: &str) -> Option<&'a Value> {
        if let Some(value) = &self.value
            && let Some(found) = value.get(key)
        {
            return Some(found);
        }
        if let Some(Value::Map(entries)) = &self.args
            && let Some((_, val)) = entries.iter().find(|(k, _)| k == key)
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
        if let Some(Value::Map(entries)) = &mut self.args
            && let Some((_, val)) = entries.iter_mut().find(|(k, _)| k == key)
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
        if let Some(Value::Map(entries)) = &mut self.args
            && let Some((_, val)) = entries.iter_mut().find(|(k, _)| k == key)
        {
            *val = new_val;
            return;
        }
        if let Some(value @ Group(_)) = &mut self.value {
            value.push_pair(key, new_val);
        } else if let Some(Value::Map(entries)) = &mut self.args {
            entries.push((key.to_string(), new_val));
        } else if self.value.is_none() && self.args.is_none() {
            self.args = Some(Value::Map(vec![(key.to_string(), new_val)]));
        } else if self.args.is_some() {
            self.value = Some(Group(vec![tomet_ast::Entry::Pair(
                key.to_string(),
                new_val,
            )]));
        } else {
            self.args = Some(Value::Map(vec![(key.to_string(), new_val)]));
        }
    }

    fn rename_prop_key(&mut self, old_key: &str, new_key: &str) -> bool {
        let mut changed = false;
        if let Some(Value::Map(entries)) = &mut self.args {
            for (k, _) in entries.iter_mut() {
                if k == old_key {
                    *k = new_key.to_string();
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
        if let Some(Value::Map(entries)) = &mut self.args {
            for (k, val) in entries.iter_mut() {
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
        if let Some(Value::Map(entries)) = &mut self.args
            && let Some(idx) = entries.iter().position(|(k, _)| k == key)
        {
            return Some(entries.remove(idx).1);
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
/// reads for, and silently dropping it here would repeat the exact bug
/// `connects` used to have (a full-form list item's parsed connects were
/// read and then thrown away at this exact spot).
#[allow(clippy::too_many_arguments)]
pub fn list_item(
    name: Option<Name>,
    content: Vec<Block>,
    args: Option<Value>,
    attrs: Option<Value>,
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
