
#[derive(Debug, serde::Deserialize)]
pub struct ConsensusEdge {
    pub id: u32,
    pub label: String,
    pub left: u32,
    pub right: u32,
    pub weight: f32,
}


#[derive(Debug, serde::Deserialize)]
pub struct ConsensusGoal {
    pub id: u32,
    pub plugged: bool,
    pub st8mnt: String,
    pub weight: f32,
}
