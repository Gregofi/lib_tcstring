use crate::decode::model::TcModelV2;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use bitvec::prelude::*;

trait AsVector {
    fn push_bit(&mut self, value: bool);
    fn push_u8(&mut self, value: u8);
    fn push_u16(&mut self, value: u16);
    fn push_12bit(&mut self, value: u16);
    fn push_11bit(&mut self, value: u16);
}

impl AsVector for BitVec<u8, Msb0> {
    fn push_bit(&mut self, value: bool) {
        self.push(value);
    }

    fn push_u8(&mut self, value: u8) {
        self.extend_from_bitslice(&value.view_bits::<Msb0>());
    }

    fn push_u16(&mut self, value: u16) {
        self.extend_from_bitslice(&value.view_bits::<Msb0>());
    }

    fn push_12bit(&mut self, value: u16) {
        assert!(value >> 12 == 0, "Value must be 12 bits or less");
        self.extend_from_bitslice(&value.view_bits::<Msb0>()[4..16]);
    }

    fn push_11bit(&mut self, value: u16) {
        assert!(value >> 11 == 0, "Value must be 12 bits or less");
        self.extend_from_bitslice(&value.view_bits::<Msb0>()[5..16]);
    }
}

fn encode_bitfield_or_range(v: &[u16], out: &mut BitVec<u8, Msb0>) {
    // TODO: Actually make a decision on some measurement
    if v.len() < 10 {
        println!("Using bitfield encoding for {:?}", v);
        encode_as_bitfield(v, out)
    } else {
        // Use range encoding
        encode_as_range(v, out)
    }
}

fn encode_as_bitfield(v: &[u16], out: &mut BitVec<u8, Msb0>) {
    let largest: u16 = *v.iter().max().unwrap_or(&0);
    out.push_u16(largest);

    out.push_bit(false); // BitfieldEntry

    let len = v.len();
    // Fill the bitfield with 0s
    out.resize(largest as usize, false);

    for id in v {
        out.set(len + *id as usize - 1, true);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryValue {
    Single(u16),
    Range(u16, u16),
}

/// Converts a vector of vendor IDs into a vector of EntryValue,
/// which have compressed consequtive vendor IDs into ranges.
fn into_intervals(v: &[u16]) -> Vec<EntryValue> {
    if v.is_empty() {
        return vec![];
    }

    let mut result = vec![];

    let mut start = 0;
    let mut end = 1;

    loop {
        while end < v.len() && v[end - 1] + 1 == v[end] {
            end += 1;
        }

        result.push(if start == end - 1 {
            EntryValue::Single(v[start])
        } else {
            EntryValue::Range(v[start], v[end - 1])
        });

        if end >= v.len() {
            break;
        }

        start = end;
        end += 1;
    }

    return result;
}

fn encode_as_range(ids: &[u16], out: &mut BitVec<u8, Msb0>) {
    // MaxVendorId
    let largest: u16 = *ids.iter().max().unwrap_or(&0);
    out.push_u16(largest);
    out.push_bit(true);

    let intervals = into_intervals(ids);
    out.push_12bit(intervals.len() as u16);
    // RangeEntry
    // Each range entry is either encoded as a single vendor ID or as a range of vendor IDS.
    // This is mainly if we have ids 7-12, we don't want to encode each one, but just the start and
    // end.
    for interval in intervals {
        match interval {
            EntryValue::Single(id) => {
                out.push_bit(false);
                out.push_u16(id);
            }
            EntryValue::Range(start, end) => {
                out.push_bit(true);
                out.push_u16(start);
                out.push_u16(end);
            }
        }
    }
}

trait Encode {
    fn encode(&self, out: &mut BitVec<u8, Msb0>);
}

impl Encode for TcModelV2 {
    fn encode(&self, out: &mut BitVec<u8, Msb0>) {
        out.resize(213, false);
        // CMP version (always 2)
        out[0..6].store_be(2);
        // Created
        out[6..42].store_be(self.created_at / 100);
        // LastUpdated
        out[42..78].store_be(self.updated_at / 100);
        // CmpId
        out[78..90].store_be(self.cmp_id);
        // CmpVersion
        out[90..102].store_be(self.cmp_version);
        // ConsentScreen
        out[102..108].store_be(self.consent_screen);
        // ConsentLanguage
        let lang = self.consent_language.as_bytes();
        out[108..114].store_be(lang[0] - 'A' as u8);
        out[114..120].store_be(lang[1] - 'A' as u8);
        // VendorListVersion
        println!("VendorListVersion: {}", self.vendor_list_version);
        out[120..132].store_be(self.vendor_list_version);
        // TcfPolicyVersion
        out[132..138].store_be(self.tcf_policy_version);
        // IsServiceSpecific
        out[138..139].store_be(self.is_service_specific as u8);
        // UseNonStandardTexts
        out[139..140].store_be(self.use_non_standard_stacks as u8);
        // SpecialFeatureOptIns
        for c in &self.special_feature_opt_ins {
            *out.get_mut(140 + *c as usize - 1).unwrap() = true;
        }
        // PurposesConsent
        for c in &self.purposes_consent {
            *out.get_mut(152 + *c as usize - 1).unwrap() = true;
        }
        // PurposesLITransparency
        for c in &self.purposes_li_transparency {
            *out.get_mut(176 + *c as usize - 1).unwrap() = true;
        }
        // PurposeOneTreatment
        out[200..201].store_be(self.purpose_one_treatment as u8);
        // PublisherCC
        let lang = self.publisher_country_code.as_bytes();
        out[201..207].store_be(lang[0] - 'A' as u8);
        out[207..213].store_be(lang[1] - 'A' as u8);
        // VendorConsent Section
        println!("VendorsConsent: {:?}", self.vendors_consent);
        encode_bitfield_or_range(&self.vendors_consent, out);
        //// VendorLegitimateInterests Section
        println!("VendorsLIConsent: {:?}", self.vendors_li_consent);
        encode_bitfield_or_range(&self.vendors_li_consent, out);
        //// PublisherRestrictions Section
        //// TODO:
        out.push_12bit(0);
    }
}

pub fn encode(tc_model: &TcModelV2) -> String {
    let mut out: BitVec<u8, Msb0> = BitVec::new();
    tc_model.encode(&mut out);
    println!("{:?}", &out[6..38]);
    out.set_uninitialized(false);
    let bytes: Vec<u8> = out.into_vec();
    URL_SAFE.encode(&bytes)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::TcModelV2;

    #[test]
    fn test_encode() {
        let tc_string = "CQNYlAAQNYlAAD3ACQCSBvFsAP_gAEPgAATIJNQJgAFAAQAAqABkAEAAKAAZAA0ACSAEwAJwAWwAvwBhAGIAQEAggCEAEUAI4ATgAoQBxADuAIQAUgA04COgE2gKkAVkAtwBeYDGQGWAMuAf4BAcCMwEmgSrgLAAVABAADIAGgATAAxAB-AEIAI4ATgAzQB3AEIAIsAm0BUgCsgFuALzAZYAy4CVYAAA.YAAAAAAAAWAA";
        let tc_model = TcModelV2::try_from(tc_string).unwrap();
        println!("created {}", tc_model.created_at);
        let encoded = super::encode(&tc_model);
        println!("{}", encoded);
        assert_eq!(tc_string, encoded);
    }

    #[test]
    fn test_intervals() {
        assert_eq!(into_intervals(&[0]), [EntryValue::Single(0)]);
        assert_eq!(
            into_intervals(&[0, 2]),
            [EntryValue::Single(0), EntryValue::Single(2)]
        );
        assert_eq!(into_intervals(&[0, 1]), [EntryValue::Range(0, 1)]);
        assert_eq!(into_intervals(&[0, 1, 2]), [EntryValue::Range(0, 2)]);
        assert_eq!(into_intervals(&[0, 1, 2, 3]), [EntryValue::Range(0, 3)]);
        assert_eq!(
            into_intervals(&[0, 1, 2, 3, 5]),
            [EntryValue::Range(0, 3), EntryValue::Single(5)]
        );
        assert_eq!(
            into_intervals(&[0, 1, 2, 5, 7, 8, 9, 11]),
            [
                EntryValue::Range(0, 2),
                EntryValue::Single(5),
                EntryValue::Range(7, 9),
                EntryValue::Single(11),
            ]
        );
    }
}
