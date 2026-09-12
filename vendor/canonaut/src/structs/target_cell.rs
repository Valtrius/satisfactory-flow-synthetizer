/// One node of a target-cell chain: an owned bitset (`cell_data`) plus a link
/// to the next node in the pool, replacing the original malloc'd/pointer-linked C struct.
#[derive(Debug, Clone)]
pub struct TargetCellNode {
    pub cell_data: Vec<usize>,
    pub next_index: Option<usize>,
}

impl TargetCellNode {
    pub fn new(capacity: usize) -> Self {
        Self {
            cell_data: vec![0; capacity],
            next_index: None,
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut usize {
        self.cell_data.as_mut_ptr()
    }

    pub fn ensure_capacity(&mut self, min_capacity: usize) {
        if self.cell_data.len() < min_capacity {
            self.cell_data.resize(min_capacity, 0)
        }
    }
}

/// Pure Rust target cell node pool — bump allocator.
///
/// Nodes are allocated in depth-first order (root=0, depth-1 child=1, ...).
/// Call `reset()` at the start of each canonicalize() invocation so the same O(depth)
/// nodes are reused without any heap allocation on the hot path.
#[derive(Debug, Default)]
pub struct TargetCellNodePool {
    /// All nodes, indexed 0..next_alloc
    nodes: Vec<TargetCellNode>,
    /// High-water mark: nodes[0..next_alloc] are "allocated" this run
    next_alloc: usize,
}

impl TargetCellNodePool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the pool for a new canonicalize() call.  O(1): just resets the bump pointer.
    /// Existing nodes are reused; their next_index is cleared on first allocation.
    #[inline]
    pub fn reset(&mut self) {
        self.next_alloc = 0;
    }

    /// Bump-allocate the next node.  Reuses an existing Vec entry when possible;
    /// only calls push() when the pool needs to grow.  next_index is always cleared
    /// so callers see a fresh node.
    #[inline]
    pub fn allocate_node(&mut self, capacity: usize) -> usize {
        let pool_index = self.next_alloc;
        self.next_alloc += 1;
        if pool_index < self.nodes.len() {
            let node = &mut self.nodes[pool_index];
            node.next_index = None;
            node.ensure_capacity(capacity);
        } else {
            self.nodes.push(TargetCellNode::new(capacity));
        }
        pool_index
    }

    pub fn get_node_mut(&mut self, index: usize) -> Option<&mut TargetCellNode> {
        self.nodes.get_mut(index)
    }

    /// # Safety
    /// `index` must be < `self.nodes.len()`.
    pub unsafe fn get_node_unchecked_mut(&mut self, index: usize) -> &mut TargetCellNode {
        unsafe { self.nodes.get_unchecked_mut(index) }
    }

    pub fn get_cell_data_ptr(&mut self, index: usize) -> *mut usize {
        if let Some(node) = self.get_node_mut(index) {
            node.as_mut_ptr()
        } else {
            std::ptr::null_mut()
        }
    }

    pub fn link_nodes(&mut self, parent_index: usize, child_index: usize) {
        if let Some(parent) = self.get_node_mut(parent_index) {
            parent.next_index = Some(child_index);
        }
    }

    pub fn get_next_index(&self, index: usize) -> Option<usize> {
        self.nodes.get(index).and_then(|node| node.next_index)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
