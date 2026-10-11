//! Allocation-free raw accounting before fallible conversion and domain indexes.

use super::{
    d, wire, TeletextAuthoredLocation as Location, TeletextAuthoredPagesError as Error,
    TeletextAuthoredSection as Section,
};

fn charge(
    remaining: &mut usize,
    amount: usize,
    dimension: &'static str,
    maximum: usize,
    location: Location,
) -> Result<(), Error> {
    *remaining = remaining.checked_sub(amount).ok_or(Error::Budget {
        location,
        dimension,
        maximum,
    })?;
    Ok(())
}

fn fence(
    amount: usize,
    maximum: usize,
    dimension: &'static str,
    section: Section,
) -> Result<(), Error> {
    let mut remaining = maximum;
    charge(
        &mut remaining,
        amount,
        dimension,
        maximum,
        Location::new(section, maximum.min(amount)),
    )
}

pub(super) fn preflight(
    input: &wire::TeletextPagesV2Response,
    service: d::TeletextServiceBudget,
    declared: d::TeletextDeclaredServiceBudget,
) -> Result<(), Error> {
    let mut records = service.max_records;
    for (section, count) in [
        (Section::Base, input.pages.len()),
        (Section::Drcs, input.drcs.len()),
        (Section::Companions, input.empty_object_pages.len()),
    ] {
        charge(
            &mut records,
            count,
            "record",
            service.max_records,
            Location::new(section, 0),
        )?;
    }
    fence(
        input.objects.len(),
        service.max_objects,
        "object",
        Section::Objects,
    )?;
    for (section, count, maximum) in [
        (Section::Mots, input.mots.len(), declared.max_mots),
        (Section::Base, input.pages.len(), declared.max_displays),
        (
            Section::Pages,
            input.page_enhancements.len(),
            declared.max_displays,
        ),
        (
            Section::Selections,
            input.mot_selections.len(),
            declared.max_displays,
        ),
        (
            Section::Companions,
            input.empty_object_pages.len(),
            declared.max_resources,
        ),
        (Section::Drcs, input.drcs.len(), declared.max_resources),
        (Section::Objects, input.objects.len(), declared.max_objects),
    ] {
        fence(count, maximum, "declaration record", section)?;
    }
    base_structure(input, service)?;
    commands(input, service.max_commands)?;
    declarations(input, declared.max_defaults)
}

fn base_structure(
    input: &wire::TeletextPagesV2Response,
    budget: d::TeletextServiceBudget,
) -> Result<(), Error> {
    let mut items = budget.max_structure_items;
    let mut bytes = budget.max_base_content_bytes;
    for (section, count) in [
        (Section::Base, input.pages.len()),
        (Section::Pages, input.page_enhancements.len()),
        (Section::Magazines, input.magazines.len()),
        (Section::Drcs, input.drcs.len()),
        (Section::Companions, input.empty_object_pages.len()),
    ] {
        charge(
            &mut items,
            count,
            "structure item",
            budget.max_structure_items,
            Location::new(section, 0),
        )?;
    }
    for (index, page) in input.pages.iter().enumerate() {
        let location = Location::new(Section::Base, index);
        charge(
            &mut items,
            page.rows.len(),
            "structure item",
            budget.max_structure_items,
            location,
        )?;
        charge(
            &mut items,
            page.navigation.len(),
            "structure item",
            budget.max_structure_items,
            location,
        )?;
        for row in &page.rows {
            charge(
                &mut items,
                row.runs.len(),
                "structure item",
                budget.max_structure_items,
                location,
            )?;
            for run in &row.runs {
                let count = match &run.content {
                    wire::TeletextRunContentWire::Text { text } => text.len(),
                    wire::TeletextRunContentWire::Mosaics { cells, .. } => cells.len(),
                };
                charge(
                    &mut bytes,
                    count,
                    "base content byte",
                    budget.max_base_content_bytes,
                    location,
                )?;
            }
        }
    }
    for (index, table) in input.drcs.iter().enumerate() {
        charge(
            &mut items,
            table.glyphs.len(),
            "structure item",
            budget.max_structure_items,
            Location::new(Section::Drcs, index),
        )?;
    }
    Ok(())
}

fn commands(input: &wire::TeletextPagesV2Response, maximum: usize) -> Result<(), Error> {
    let mut remaining = maximum;
    for (index, page) in input.page_enhancements.iter().enumerate() {
        let location = Location::new(Section::Pages, index);
        if let Some(program) = &page.local_program {
            program_commands(program, &mut remaining, maximum, location)?;
        }
        charge(
            &mut remaining,
            page.resources.defaults.iter().flatten().count(),
            "authored command",
            maximum,
            location,
        )?;
    }
    for (index, definition) in input.objects.iter().enumerate() {
        program_commands(
            &definition.program,
            &mut remaining,
            maximum,
            Location::new(Section::Objects, index),
        )?;
    }
    Ok(())
}

fn program_commands(
    program: &wire::TeletextEnhancementProgramWire,
    remaining: &mut usize,
    maximum: usize,
    location: Location,
) -> Result<(), Error> {
    let offender = *remaining;
    charge(
        remaining,
        program.commands.len(),
        "authored command",
        maximum,
        Location {
            command: Some(offender),
            ..location
        },
    )
}

fn declarations(input: &wire::TeletextPagesV2Response, maximum: usize) -> Result<(), Error> {
    let mut remaining = maximum;
    for (index, mot) in input.mots.iter().enumerate() {
        let location = Location::new(Section::Mots, index);
        if mot.associations.len() > 256 {
            return Err(Error::Budget {
                location,
                dimension: "MOT association",
                maximum: 256,
            });
        }
        for link in std::iter::once(&mot.links.global_object).chain(&mot.links.public_objects) {
            let count = link
                .defaults
                .iter()
                .filter(|value| matches!(value, wire::TeletextMotDefaultObjectWire::Object { .. }))
                .count();
            charge(&mut remaining, count, "declared default", maximum, location)?;
        }
    }
    for (index, table) in input.drcs.iter().enumerate() {
        if table.glyphs.len() > 48 {
            return Err(Error::Budget {
                location: Location::new(Section::Drcs, index),
                dimension: "DRCS glyph",
                maximum: 48,
            });
        }
    }
    Ok(())
}
