use acacia_physics::registry_world::RegistryWorld;
use acacia_world::ChunkView;

/// The tracked terrain as the physics engine sees it.
pub type PhysicsWorld<'a> = RegistryWorld<'a, ChunkView>;
