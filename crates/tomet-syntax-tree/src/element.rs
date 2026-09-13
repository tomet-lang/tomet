//! Extension trait, attribute helpers, and constructors for [`tomet_ast::Element`].

use tomet_ast::{Block, Element, ElementValue, Inline, Name, Placement, Sigil, Span, Value};

/// Extension trait providing accessors, attribute manipulations, and inspections on [`Element`].
pub trait ElementExt {
    /// Consumes `self` and sets its `span`.
    fn with_span(self, span: Span) -> Self;

    /// Consumes `self` and sets its `placement`.
    fn with_placement(self, placement: Placement) -> Self;

    /// Consumes `self` and sets its `args`.
    fn with_args(self, args: Value) -> Self;

    /// Consumes `self` and sets its `content`.
    fn with_content(self, content: Vec<Inline>) -> Self;

    /// Consumes `self` and sets its `children`.
    fn with_children(self, children: Vec<Block>) -> Self;

    /// Consumes `self` and sets its `value`.
    fn with_value(self, value: ElementValue) -> Self;

    /// Returns the element's name, namespace included, if it has one.
    fn name(&self) -> Option<&Name>;

    /// Returns just the local half of the element's name, without its
    /// namespace. Prefer [`ElementExt::name`] or
    /// [`tomet_ast::Sigil::is_bare_named`] when the namespace matters.
    fn local_name(&self) -> Option<&str>;

    /// Returns `true` if this element has `Sigil::Bare`.
    fn is_bare(&self) -> bool;

    /// Merges an [`Element`]'s `args` and the `key: value` pairs of its
    /// value group into one attrs view -- the value group's keys win on
    /// conflict.
    fn attrs_view(&self) -> Option<Value>;

    /// Mutable writable attrs slot on `el.args`.
    ///
    /// Unlike [`ElementExt::attrs_view`] this does not fall back to the
    /// value group: a group is a list of [`tomet_ast::Entry`] items, not a
    /// `Value`, so there is no single `&mut Value` to hand out for it. Use
    /// [`ElementExt::set_prop`] or `ElementValue`'s own pair accessors to
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
        self.args = Some(args);
        self
    }

    fn with_content(mut self, content: Vec<Inline>) -> Self {
        self.content = Some(content);
        self
    }

    fn with_children(mut self, children: Vec<Block>) -> Self {
        self.children = Some(children);
        self
    }

    fn with_value(mut self, value: ElementValue) -> Self {
        self.value = Some(value);
        self
    }

    fn name(&self) -> Option<&Name> {
        self.sigil.name()
    }

    fn local_name(&self) -> Option<&str> {
        self.sigil.name().map(|n| n.name.as_str())
    }

    fn is_bare(&self) -> bool {
        matches!(self.sigil, Sigil::Bare)
    }

    fn attrs_view(&self) -> Option<Value> {
        let args_map = match &self.args {
            Some(Value::Map(entries)) => Some(entries.clone()),
            _ => None,
        };
        let value_map = self.value.as_ref().and_then(|v| match v.as_data() {
            Some(Value::Map(entries)) => Some(entries),
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
        if let Some(value) = &self.value {
            if let Some(found) = value.get(key) {
                return Some(found);
            }
        }
        if let Some(Value::Map(entries)) = &self.args {
            if let Some((_, val)) = entries.iter().find(|(k, _)| k == key) {
                return Some(val);
            }
        }
        None
    }

    fn get_attr_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut Value> {
        // Checked separately from the mutable borrow below so `value` and
        // `args` are never borrowed mutably at the same time.
        if self.value.as_ref().is_some_and(|v| v.get(key).is_some()) {
            return self.value.as_mut().and_then(|v| v.get_mut(key));
        }
        if let Some(Value::Map(entries)) = &mut self.args {
            if let Some((_, val)) = entries.iter_mut().find(|(k, _)| k == key) {
                return Some(val);
            }
        }
        None
    }

    fn has_prop_key(&self, key: &str) -> bool {
        self.get_attr(key).is_some()
    }

    fn set_prop(&mut self, key: &str, new_val: Value) {
        if let Some(value) = &mut self.value {
            if let Some(slot) = value.get_mut(key) {
                *slot = new_val;
                return;
            }
        }
        if let Some(Value::Map(entries)) = &mut self.args {
            if let Some((_, val)) = entries.iter_mut().find(|(k, _)| k == key) {
                *val = new_val;
                return;
            }
        }
        if let Some(value @ ElementValue::Group(_)) = &mut self.value {
            value.push_pair(key, new_val);
        } else if let Some(Value::Map(entries)) = &mut self.args {
            entries.push((key.to_string(), new_val));
        } else if self.value.is_none() && self.args.is_none() {
            self.args = Some(Value::Map(vec![(key.to_string(), new_val)]));
        } else if self.args.is_some() {
            self.value = Some(ElementValue::Group(vec![tomet_ast::Entry::Pair(
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
        if let Some(value) = &mut self.value {
            if let Some(removed) = value.remove(key) {
                return Some(removed);
            }
        }
        if let Some(Value::Map(entries)) = &mut self.args {
            if let Some(idx) = entries.iter().position(|(k, _)| k == key) {
                return Some(entries.remove(idx).1);
            }
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

/// Creates a new [`Element`] with given `sigil` and default empty fields.
///
/// The placement is [`Placement::Inline`]; a caller building a block uses
/// [`ElementExt::with_placement`], and the parser sets it from position.
pub fn element_new(sigil: Sigil) -> Element {
    Element {
        sigil,
        placement: Placement::Inline,
        args: None,
        content: None,
        children: None,
        value: None,
        connects: Vec::new(),
        span: Span::default(),
    }
}

/// A list element constructor (`ol` if `ordered`, else `ul`).
///
/// A list has no sigil in the source -- the marker (`-` / `-.`) is its
/// surface form -- but it always stands as a block, so it is constructed
/// with [`Placement::Block`] directly.
pub fn element_list(ordered: bool, items: Vec<Element>, span: Span) -> Element {
    Element {
        sigil: Sigil::named(if ordered { "ol" } else { "ul" }),
        placement: Placement::Block,
        args: None,
        content: None,
        children: None,
        value: Some(ElementValue::from_children(items)),
        connects: Vec::new(),
        span,
    }
}

/// A list item element constructor (`Sigil::Bare`).
pub fn element_list_item(
    content: Vec<Inline>,
    marker: Option<Value>,
    attrs: Option<Value>,
    children: Vec<Block>,
    span: Span,
) -> Element {
    Element {
        sigil: Sigil::Bare,
        placement: Placement::Block,
        args: marker,
        content: Some(content),
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
        value: attrs.map(ElementValue::from_map),
        connects: Vec::new(),
        span,
    }
}
