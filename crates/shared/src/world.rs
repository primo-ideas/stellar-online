//! World model shared by server and client: sectors, ships and their blocks.

use serde::{Deserialize, Serialize};

/// Integer coordinates of a sector in the galaxy grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Sector {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl std::fmt::Display for Sector {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

/// Where a player is and where they look. `pos` is in sector space (metres, 1 block = 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub sector: Sector,
    pub pos: [f32; 3],
    /// Horizontal look angle, radians.
    pub yaw: f32,
    /// Vertical look angle, radians, within ±`MAX_PITCH`.
    pub pitch: f32,
}

impl PlayerState {
    pub const MAX_PITCH: f32 = 1.55;
    /// Players cannot go further than this from the sector centre.
    pub const SECTOR_HALF_SIZE: f32 = 1000.0;

    /// Rejects NaN/infinite values and out-of-range positions.
    pub fn is_valid(&self) -> bool {
        self.pos
            .iter()
            .all(|c| c.is_finite() && c.abs() <= Self::SECTOR_HALF_SIZE)
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.pitch.abs() <= Self::MAX_PITCH
    }
}

impl Default for PlayerState {
    /// Spawn point: a few blocks in front of the sector centre, looking at it.
    fn default() -> Self {
        PlayerState {
            sector: Sector::default(),
            pos: [6.0, 3.0, 8.0],
            yaw: -2.2,
            pitch: -0.3,
        }
    }
}

/// Position of a block inside a ship, relative to its core.
pub type BlockPos = [i32; 3];

/// Kinds of blocks a ship can be built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockKind {
    /// The ship's heart: exactly one per ship, cannot be removed.
    Core,
    Hull,
    Thruster,
    Reactor,
}

impl BlockKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "core" => Some(Self::Core),
            "hull" => Some(Self::Hull),
            "thruster" => Some(Self::Thruster),
            "reactor" => Some(Self::Reactor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub pos: BlockPos,
    pub kind: BlockKind,
}

/// A ship: an entity made of blocks, located in a sector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ship {
    pub id: u64,
    pub name: String,
    pub owner: String,
    pub sector: Sector,
    pub blocks: Vec<Block>,
}

impl Ship {
    /// A new ship is a single core block at the origin.
    pub fn new(id: u64, name: &str, owner: &str, sector: Sector) -> Self {
        Ship {
            id,
            name: name.to_string(),
            owner: owner.to_string(),
            sector,
            blocks: vec![Block {
                pos: [0, 0, 0],
                kind: BlockKind::Core,
            }],
        }
    }

    pub fn block_at(&self, pos: BlockPos) -> Option<&Block> {
        self.blocks.iter().find(|b| b.pos == pos)
    }

    /// Adds a block. It must touch an existing block (face adjacency), sit on a free
    /// cell, and not be a second core.
    pub fn place(&mut self, pos: BlockPos, kind: BlockKind) -> Result<(), String> {
        if kind == BlockKind::Core {
            return Err("a ship has exactly one core".into());
        }
        if self.block_at(pos).is_some() {
            return Err(format!("{pos:?} is already occupied"));
        }
        let touches = self.blocks.iter().any(|b| manhattan(b.pos, pos) == 1);
        if !touches {
            return Err(format!("{pos:?} does not touch the ship"));
        }
        self.blocks.push(Block { pos, kind });
        Ok(())
    }

    /// Removes a block. The core cannot be removed.
    /// (Splitting a ship into disconnected parts is not handled yet.)
    pub fn remove(&mut self, pos: BlockPos) -> Result<BlockKind, String> {
        let i = self
            .blocks
            .iter()
            .position(|b| b.pos == pos)
            .ok_or_else(|| format!("no block at {pos:?}"))?;
        if self.blocks[i].kind == BlockKind::Core {
            return Err("the core cannot be removed".into());
        }
        Ok(self.blocks.swap_remove(i).kind)
    }
}

fn manhattan(a: BlockPos, b: BlockPos) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_rules() {
        let mut s = Ship::new(1, "Test", "alice", Sector::default());
        assert!(s.place([1, 0, 0], BlockKind::Hull).is_ok());
        assert!(s.place([2, 0, 0], BlockKind::Thruster).is_ok());
        assert!(s.place([2, 0, 0], BlockKind::Hull).is_err()); // occupied
        assert!(s.place([5, 5, 5], BlockKind::Hull).is_err()); // floating
        assert!(s.place([0, 1, 0], BlockKind::Core).is_err()); // second core
        assert!(s.remove([0, 0, 0]).is_err()); // core
        assert_eq!(s.remove([2, 0, 0]), Ok(BlockKind::Thruster));
        assert_eq!(s.blocks.len(), 2);
    }
}
