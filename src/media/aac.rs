// AAC: AudioSpecificConfig (the two-byte codec setup stored in MP4/MKV) and ADTS frame headers
// (encoder output, AAC in some AVI files).

pub const RATES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];

pub fn rate_index(rate: u32) -> u8 {
    RATES.iter().position(|&r| r == rate).unwrap_or(4) as u8
}

/// AudioSpecificConfig: object type (2 = AAC LC), sampling rate, channels.
pub fn asc(object_type: u8, rate: u32, channels: u16) -> Vec<u8> {
    let fi = rate_index(rate);
    let ch = channels.min(7) as u8;
    vec![(object_type << 3) | (fi >> 1), ((fi & 1) << 7) | (ch << 3)]
}

/// AudioSpecificConfig → (object type, sampling rate, channels).
pub fn parse_asc(asc: &[u8]) -> Option<(u8, u32, u16)> {
    if asc.len() < 2 {
        return None;
    }
    let object = asc[0] >> 3;
    let fi = ((asc[0] & 7) << 1) | (asc[1] >> 7);
    let rate = *RATES.get(fi as usize)?;
    let ch = ((asc[1] >> 3) & 0x0F) as u16;
    Some((object, rate, ch))
}

/// If `data` starts with an ADTS header: (AudioSpecificConfig, header length).
pub fn parse_adts(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    if data.len() < 7 || data[0] != 0xFF || data[1] & 0xF6 != 0xF0 {
        return None;
    }
    let profile = (data[2] >> 6) + 1;
    let fi = (data[2] >> 2) & 0x0F;
    let ch = (((data[2] & 1) << 2) | (data[3] >> 6)) as u16;
    let rate = *RATES.get(fi as usize)?;
    let header = if data[1] & 1 == 0 { 9 } else { 7 };
    Some((asc(profile, rate, ch), header))
}
