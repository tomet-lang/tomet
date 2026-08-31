//! Extension trait and block manipulation helpers for [`tomet_ast::Document`].

use tomet_ast::{Block, Document, Element};

/// Extension trait providing block-level operations on [`Document`].
pub trait DocumentExt {
    /// Inserts `block` at `index` in `self.blocks`.
    fn insert_block(&mut self, index: usize, block: Block);

    /// Removes and returns the [`Block`] at `index` in `self.blocks`, if present.
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

impl DocumentExt for Document {
    fn insert_block(&mut self, index: usize, block: Block) {
        if index >= self.blocks.len() {
            self.blocks.push(block);
        } else {
            self.blocks.insert(index, block);
        }
    }

    fn remove_block(&mut self, index: usize) -> Option<Block> {
        if index < self.blocks.len() {
            Some(self.blocks.remove(index))
        } else {
            None
        }
    }

    fn replace_block(&mut self, index: usize, new_block: Block) -> Option<Block> {
        if index < self.blocks.len() {
            let old = std::mem::replace(&mut self.blocks[index], new_block);
            Some(old)
        } else {
            None
        }
    }

    fn find_block_index<F>(&self, predicate: F) -> Option<usize>
    where
        F: FnMut(&Block) -> bool,
    {
        self.blocks.iter().position(predicate)
    }

    fn find_element_block_index<F>(&self, mut predicate: F) -> Option<usize>
    where
        F: FnMut(&Element) -> bool,
    {
        self.blocks.iter().position(|b| match b {
            Block::Element(el) => predicate(el),
            _ => false,
        })
    }
}
