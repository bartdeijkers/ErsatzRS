//! Explicit authored-page feeding, not carousel/ordinary/client acceptance.
//! Reuse the frozen Polish helper and unchanged pinned decoder; the older
//! English private-charset limitation calibration remains a separate test.

use super::*;

const ORACLE: &str = r"
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True
path = Path(sys.argv[1])
with path.open('rb') as source:
    body = source.read(65537)
if len(body) > 65536 or hashlib.sha256(body).hexdigest() != '2887bf80a370636d81126ac00ac90770e62fb3f5606a341027841ddeb93ee884':
    raise RuntimeError('Frozen national-carrier helper differs')
spec = importlib.util.spec_from_file_location('english_resource_fixed_oracle', path)
fixed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixed)
imager, require = fixed.imager, fixed.require

def main():
    raw = sys.stdin.buffer.read(imager.resource.MAX_INPUT + 1)
    require(0 < len(raw) <= imager.resource.MAX_INPUT, 'Input byte bound exceeded')
    cases = json.loads(raw, object_pairs_hook=imager.resource.unique_object)
    require(type(cases) is list and len(cases) == 2, 'Exactly two fixed originals required')
    reader_type, decoder_type = imager.load_decoder()
    drcs_type = sys.modules['teletextimager.teletextdrcs'].TeletextDRCSDecode
    count = 0
    for underlined, case in zip((False, True), cases):
        require(type(case) is dict and set(case) == {'underlined', 'pages', 'negative_bank'}
                and type(case['underlined']) is bool and case['underlined'] == underlined,
                'Invalid fixed English case')
        require(type(case['pages']) is list and len(case['pages']) == 2,
                'Exactly the complete authored display and generated bank required')
        for units in [*case['pages'], case['negative_bank']]:
            require(type(units) is list and 1 <= len(units) <= 128, 'Invalid page-unit bound')
            count += len(units)
            require(count <= 512, 'Aggregate unit bound exceeded')
            for unit in units:
                require(type(unit) is list and len(unit) == 46 and unit[:2] == [2, 44]
                        and unit[3] == 0xE4
                        and all(type(value) is int and 0 <= value <= 255 for value in unit),
                        'Invalid actual DVB unit')

        def page(units):
            # These are explicitly supplied COMPLETE individual authored pages.
            # EOF is consumed as such, never relabelled a closed carousel feed.
            data = bytes(imager.resource.BIT_REVERSE[value]
                         for unit in units for value in unit[4:])
            stream = io.BytesIO(data)
            decoded = reader_type().read(stream)
            require(type(decoded) is list and len(decoded) == 1 and decoded[0]
                    and stream.tell() == len(data), 'Incomplete explicit authored-page feed')
            return decoded[0]

        display, bank = [page(units) for units in case['pages']]
        require(display.get('number') == 0x200 and display.get('subcode') == 0x1234
                and 1 in display and 2 in display and 3 in display and (26, 0) in display,
                'Fixed display identity/body differs')
        slot = 8 + int(underlined)
        mask = [0, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198,
                0xFFF if underlined else 0]

        def received(units_page):
            require(units_page.get('number') == 0x1A0 and units_page.get('subcode') & 15 == 0
                    and set(units_page) == {'number', 'subcode', 'control', 0, (28, 3), 5}
                    and units_page[(28, 3)][0] & 15 == 5,
                    'Complete fixed Normal1A0/S0 bank differs')
            decoder = drcs_type()
            decoder.set_page([units_page])
            require(decoder.ptu_mode(slot, 0) == 0, 'Fixed PTU mode differs')
            return decoder

        def bank_mask(decoder):
            bitmap = decoder.ptu_l2p5(slot, 0)
            require(bitmap is not None and list(bitmap) == fixed.bitmap(mask),
                    'Received English PTU literal bitmap differs')

        bank_mask(received(bank))
        negative = page(case['negative_bank'])
        negative_decoder = received(negative)
        try:
            bank_mask(negative_decoder)
        except imager.resource.CalibrationError as error:
            require(str(error) == 'Received English PTU literal bitmap differs',
                    'Unexpected negative failure')
        else:
            raise imager.resource.CalibrationError('Valid-parity PTU pixel negative was accepted')

        for level in ('1', '1.5', '2.5'):
            decoder = decoder_type()
            decoder.decode(display, level=level, ndrcs_page=[bank])
            if level == '2.5':
                for column, fragment in ((7, 3), (8, 4)):
                    fixed.cell(decoder, 1, column, 0x7C, fragment, (0, slot, mask))
                fixed.cell(decoder, 4, 12, 65, 0)
                # Original designation37 maps to Greek set4 in the pinned decoder.
                require(decoder.get_char_set(4, 12) == 4
                        and decoder.get_char_diacritic(4, 12) == 0,
                        'Unrelated original Greek A differs')
            else:
                fixed.cell(decoder, 1, 7, 0x7C, 0)
                fixed.cell(decoder, 1, 8, 32, 0)
                fixed.cell(decoder, 2, 8, 101, 0)
                fixed.cell(decoder, 4, 12, 32, 0)
            fixed.cell(decoder, 3, 1, 66, 0)
    print(json.dumps({
        'decoder': 'teletextimager', 'version': imager.VERSION, 'commit': imager.COMMIT,
        'external_source_sha256': imager.HASHES, 'input_sha256': hashlib.sha256(raw).hexdigest(),
        'cases_passed': 2, 'levels': ['1', '1.5', '2.5'], 'fixed_slots': [8, 9],
        'literal_english_masks_and_fragments': True, 'unrelated_original_greek_checked': True,
        'valid_parity_ptu_pixel_negatives_rejected': 2, 'fallback_receiver_region': 0,
        'complete_authored_pages_explicitly_fed': True, 'carousel_acquisition_proven': False,
        'automatic_resource_routing_proven': False, 'direct_national_recipe_proven': False,
        'font_pixels_or_actual_client_proven': False, 'full_engine_acceptance': False,
    }, sort_keys=True))

try:
    main()
except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
        RecursionError, KeyError, IndexError, TypeError) as error:
    print(json.dumps({'error': str(error)}), file=sys.stderr)
    sys.exit(1)
";

fn independent_case(underlined: bool) -> serde_json::Value {
    let case = prepare(underlined);
    let before = case.input.clone();
    let encoded = encode_teletext_authored_service(
        case.input.clone(),
        &authored::transmissions(&case.input),
        &service_budget(),
    )
    .unwrap();
    assert_output(&case, &encoded);
    assert_eq!(case.input, before);
    assert!(encoded.magazine_units().is_empty());
    assert_eq!(encoded.pages().len(), 2);
    let pages = encoded
        .pages()
        .iter()
        .map(|page| {
            page.data_units()
                .iter()
                .map(|unit| unit.to_vec())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut negative = encoded.pages()[1].data_units().to_vec();
    let row = negative
        .iter_mut()
        .find(|unit| {
            matches!(crate::teletext::source_header::observe_dvb_teletext_header(unit).unwrap(),
                crate::teletext::source_header::TeletextHeaderObservation::NotHeader(address)
                if address.packet_number == 5)
        })
        .unwrap();
    let offset = 6 + 20 * usize::from(underlined);
    let old = row[offset];
    assert_eq!(old.count_ones() % 2, 1);
    assert_eq!(old.reverse_bits() & 63, 0);
    row[offset] =
        crate::teletext::packets::odd_parity((old.reverse_bits() & 0x7F) ^ 1).reverse_bits();
    assert_ne!(old, row[offset]);
    assert_eq!(row[offset].count_ones() % 2, 1);
    serde_json::json!({
        "underlined": underlined, "pages": pages,
        "negative_bank": negative.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>()
    })
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn english_two_pair_resource_carrier_decodes_with_pinned_imager() {
    let cases = [false, true].map(independent_case);
    let request = serde_json::to_vec(&cases).unwrap();
    assert!(request.len() <= 2 * 1024 * 1024);
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_ENGLISH_RESOURCE_IMAGER_FIXTURE",
    ]) {
        std::fs::write(path, &request).unwrap();
    }
    let helper = test_manifest_dir!().join("../../scripts/e2e/teletext-polish-carrier-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output = crate::teletext_probe::run_probe(
        &python,
        &["-c", ORACLE, helper.to_str().unwrap()],
        &request,
        16 * 1024,
    )
    .await
    .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["decoder"], "teletextimager");
    assert_eq!(report["version"], "0.2.0");
    assert_eq!(report["commit"], "2b0125b778c45ffdfa2df0b196870c093361f317");
    assert_eq!(report["cases_passed"], 2);
    assert_eq!(report["fixed_slots"], serde_json::json!([8, 9]));
    assert_eq!(report["valid_parity_ptu_pixel_negatives_rejected"], 2);
    assert_eq!(report["carousel_acquisition_proven"], false);
    assert_eq!(report["full_engine_acceptance"], false);
    println!("Independent explicit English two-pair resource cells: {report}");
}
