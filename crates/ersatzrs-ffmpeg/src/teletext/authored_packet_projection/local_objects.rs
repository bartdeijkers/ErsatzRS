//! Strict logical inverse of actual generated Local-object packet bodies.
use super::{
    error, exact_nibble, project_body, TeletextEncodedDisplayProjectionError,
    TeletextEncodedDisplayProjectionFailure as Failure, TeletextEncodedDisplayRecord,
};
use crate::teletext::EncodedTeletextLocalObjectProgram;

/// Observe generated Local X26 bodies without correction or reconstruction.
/// The positive packet budget and native sixteen-packet bound precede allocation.
/// Exact designation and triplet codewords are checked; this grants no editorial
/// permission or semantic equivalence for any carrier made from these records.
/// # Errors
/// Returns a located existing projection diagnostic for bounds, noncanonical
/// designation order or dirty protection. No partial records are returned.
pub fn project_encoded_teletext_local_objects(
    program: &EncodedTeletextLocalObjectProgram,
    max_packets: usize,
) -> Result<Vec<TeletextEncodedDisplayRecord>, TeletextEncodedDisplayProjectionError> {
    project_bodies(program.packet_bodies(), max_packets)
}

fn project_bodies(
    bodies: &[[u8; 40]],
    max_packets: usize,
) -> Result<Vec<TeletextEncodedDisplayRecord>, TeletextEncodedDisplayProjectionError> {
    if max_packets == 0 || bodies.is_empty() || bodies.len() > max_packets || bodies.len() > 16 {
        return Err(error(Failure::Budget, None, None, None));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact(bodies.len())
        .map_err(|_| error(Failure::Budget, None, None, None))?;
    for (index, body) in bodies.iter().enumerate() {
        let designation = exact_nibble(body[0])
            .ok_or_else(|| error(Failure::Protection, Some(index), Some(26), Some(0)))?;
        if usize::from(designation) != index {
            return Err(error(
                Failure::UnsupportedPacket,
                Some(index),
                Some(26),
                Some(0),
            ));
        }
        records.push(project_body(26, body, index)?);
    }
    Ok(records)
}

#[cfg(test)]
mod tests;
