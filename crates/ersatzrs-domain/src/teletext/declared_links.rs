//! Four-role X27 materialization from an already authenticated MOT selection.
use super::{
    TeletextDeclaredDisplayResolution, TeletextDrcsExtensionError,
    TeletextMotResolutionRequirements, TeletextMotRoleResolution, TeletextSubpageRequirementMask,
    TeletextX27Level25Links, TeletextX27LinkDeclaration, TeletextX27LinkError, TeletextX27LinkRole,
    TeletextX27LinkTarget, TeletextX27LinkValidity, TeletextX27Magazine,
};

impl TeletextDeclaredDisplayResolution {
    /// Materialize an original first-attachment context without inventing MOT authority.
    ///
    /// An absent MOT is sufficient only when every authenticated role is exactly
    /// unselected. Existing selected/MOT contexts retain their original rules.
    /// This grants no inventory, edit or publication authority.
    /// # Errors
    /// Rejects unresolved roles or any non-MOT selected role requiring X27 provenance.
    pub fn materialized_level25_attachment_links(
        &self,
    ) -> Result<TeletextX27Level25Links, TeletextDrcsExtensionError> {
        if self.mot().is_some()
            || TeletextX27LinkRole::ALL
                .iter()
                .any(|role| *self.role(*role) != TeletextMotRoleResolution::Unselected)
        {
            return self.materialized_level25_links();
        }
        materialize(self).map_err(|error| match error {
            MaterializationError::Unproven => TeletextDrcsExtensionError::UnprovenOtherRoles {
                display: self.display(),
            },
            MaterializationError::Link(error) => TeletextDrcsExtensionError::Link(error),
        })
    }

    /// Derive four X27 roles solely from this authenticated MOT resolution.
    /// This does not validate inventory, grant edit permission or authorize
    /// publication, and cannot materialize an existing X27-mask resolution.
    /// # Errors
    /// Rejects any role not proven by the original MOT extent, and invalid links.
    pub fn materialized_level25_links(
        &self,
    ) -> Result<TeletextX27Level25Links, TeletextDrcsExtensionError> {
        if self.mot().is_none_or(|mot| mot.x27().is_some()) {
            return Err(TeletextDrcsExtensionError::UnprovenOtherRoles {
                display: self.display(),
            });
        }
        materialize(self).map_err(|error| match error {
            MaterializationError::Unproven => TeletextDrcsExtensionError::UnprovenOtherRoles {
                display: self.display(),
            },
            MaterializationError::Link(error) => TeletextDrcsExtensionError::Link(error),
        })
    }
}

pub(super) enum MaterializationError {
    Unproven,
    Link(TeletextX27LinkError),
}

impl From<TeletextX27LinkError> for MaterializationError {
    fn from(value: TeletextX27LinkError) -> Self {
        Self::Link(value)
    }
}

pub(super) fn materialize(
    resolution: &TeletextDeclaredDisplayResolution,
) -> Result<TeletextX27Level25Links, MaterializationError> {
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(resolution.display().page.magazine())?,
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )?;
    let mut links = [unused; 4];
    for role in TeletextX27LinkRole::ALL {
        links[usize::from(role.wire_code())] = match *resolution.role(role) {
            TeletextMotRoleResolution::Unselected => unused,
            TeletextMotRoleResolution::Selected {
                target,
                requirements: TeletextMotResolutionRequirements::MotExtent(extent),
                ..
            } => TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page { page: target },
                TeletextX27LinkValidity::Level25Only,
                extent.required_mask(),
            )?,
            _ => return Err(MaterializationError::Unproven),
        };
    }
    Ok(TeletextX27Level25Links::new(
        links[0], links[1], links[2], links[3],
    )?)
}
