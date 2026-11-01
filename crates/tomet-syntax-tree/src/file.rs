//! Extension trait and block manipulation helpers for [`tomet_ast::File`].

use tomet_ast::{Block, Element, File};

/// Extension trait providing block-level operations on [`File`].
pub trait FileExt {
    /// Inserts `block` at `index` in `self.body`.
    fn insert_block(&mut self, index: usize, block: Block);

    /// Removes and returns the [`Block`] at `index` in `self.body`, if present.
    fn remove_block(&mut self, index: usize) -> Option<Block>;

    /// Replaces the [`Block`] at `index` with `new_block`, returning the previous block if in bounds.
    fn replace_block(&mut self, index: usize, new_block: Block) -> Option<Block>;

    /// Finds the index of the first top-level block matching `predicate`.
    fn find_block_index<F>(&self, predicate: F) -> Option<usize>
    where
        F: FnMut(&Block) -> bool;

    /// Finds the index of the first top-level element matching `predicate`.
    fn find_element_block_index<F>(&self, predicate: F) -> Option<usize>
    where
        F: FnMut(&Element) -> bool;
}

impl FileExt for File {
    fn insert_block(&mut self, index: usize, block: Block) {
        if index >= self.body.len() {
            self.body.push(block);
        } else {
            self.body.insert(index, block);
        }
    }

    fn remove_block(&mut self, index: usize) -> Option<Block> {
        if index < self.body.len() {
            Some(self.body.remove(index))
        } else {
            None
        }
    }

    fn replace_block(&mut self, index: usize, new_block: Block) -> Option<Block> {
        if index < self.body.len() {
            let old = std::mem::replace(&mut self.body[index], new_block);
            Some(old)
        } else {
            None
        }
    }

    fn find_block_index<F>(&self, predicate: F) -> Option<usize>
    where
        F: FnMut(&Block) -> bool,
    {
        self.body.iter().position(predicate)
    }

    fn find_element_block_index<F>(&self, mut predicate: F) -> Option<usize>
    where
        F: FnMut(&Element) -> bool,
    {
        self.body.iter().position(|b| match b {
            Block::Element(el) => predicate(el),
            _ => false,
        })
    }
}
