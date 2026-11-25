use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimecodeError {
    #[error("Invalid timecode format '{0}'. Expected HH:MM:SS:FF or HH:MM:SS;FF")]
    InvalidFormat(String),
    #[error("Timecode field out of range in '{0}'")]
    OutOfRange(String),
}

/// All standard DaVinci Resolve timeline frame rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameRate {
    Fps23_976,
    Fps24,
    Fps25,
    Fps29_97Ndf,
    Fps29_97Df,
    Fps30,
    Fps50,
    Fps59_94Ndf,
    Fps59_94Df,
    Fps60,
}

impl Default for FrameRate {
    fn default() -> Self {
        // User-specified default: 25 fps
        FrameRate::Fps25
    }
}

impl FrameRate {
    pub const ALL: [FrameRate; 10] = [
        FrameRate::Fps23_976,
        FrameRate::Fps24,
        FrameRate::Fps25,
        FrameRate::Fps29_97Ndf,
        FrameRate::Fps29_97Df,
        FrameRate::Fps30,
        FrameRate::Fps50,
        FrameRate::Fps59_94Ndf,
        FrameRate::Fps59_94Df,
        FrameRate::Fps60,
    ];

    /// Exact rational frame rate (numerator, denominator) in frames per real wall-clock second.
    pub fn rational(&self) -> (u64, u64) {
        match self {
            FrameRate::Fps23_976 => (24000, 1001),
            FrameRate::Fps24 => (24, 1),
            FrameRate::Fps25 => (25, 1),
            FrameRate::Fps29_97Ndf | FrameRate::Fps29_97Df => (30000, 1001),
            FrameRate::Fps30 => (30, 1),
            FrameRate::Fps50 => (50, 1),
            FrameRate::Fps59_94Ndf | FrameRate::Fps59_94Df => (60000, 1001),
            FrameRate::Fps60 => (60, 1),
        }
    }

    /// Exact floating-point frames per wall-clock second.
    pub fn fps_f64(&self) -> f64 {
        let (n, d) = self.rational();
        n as f64 / d as f64
    }

    /// Nominal integer timebase used for HH:MM:SS:FF counting.
    pub fn nominal_fps(&self) -> u64 {
        match self {
            FrameRate::Fps23_976 | FrameRate::Fps24 => 24,
            FrameRate::Fps25 => 25,
            FrameRate::Fps29_97Ndf | FrameRate::Fps29_97Df | FrameRate::Fps30 => 30,
            FrameRate::Fps50 => 50,
            FrameRate::Fps59_94Ndf | FrameRate::Fps59_94Df | FrameRate::Fps60 => 60,
        }
    }

    pub fn is_drop_frame(&self) -> bool {
        matches!(self, FrameRate::Fps29_97Df | FrameRate::Fps59_94Df)
    }

    pub fn fcm_header(&self) -> &'static str {
        if self.is_drop_frame() {
            "DROP FRAME"
        } else {
            "NON-DROP FRAME"
        }
    }

    pub fn frame_separator(&self) -> char {
        if self.is_drop_frame() {
            ';'
        } else {
            ':'
        }
    }

    /// FCPXML `frameDuration` attribute (e.g. `"100/2500s"` or `"1001/24000s"`).
    pub fn fcpxml_frame_duration(&self) -> &'static str {
        match self {
            FrameRate::Fps23_976 => "1001/24000s",
            FrameRate::Fps24 => "1/24s",
            FrameRate::Fps25 => "1/25s",
            FrameRate::Fps29_97Ndf | FrameRate::Fps29_97Df => "1001/30000s",
            FrameRate::Fps30 => "1/30s",
            FrameRate::Fps50 => "1/50s",
            FrameRate::Fps59_94Ndf | FrameRate::Fps59_94Df => "1001/60000s",
            FrameRate::Fps60 => "1/60s",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FrameRate::Fps23_976 => "23.976 fps",
            FrameRate::Fps24 => "24 fps",
            FrameRate::Fps25 => "25 fps",
            FrameRate::Fps29_97Ndf => "29.97 NDF",
            FrameRate::Fps29_97Df => "29.97 DF",
            FrameRate::Fps30 => "30 fps",
            FrameRate::Fps50 => "50 fps",
            FrameRate::Fps59_94Ndf => "59.94 NDF",
            FrameRate::Fps59_94Df => "59.94 DF",
            FrameRate::Fps60 => "60 fps",
        }
    }

    /// Convert real-world elapsed seconds into an exact integer frame count.
    pub fn seconds_to_frames(&self, seconds: f64) -> i64 {
        (seconds * self.fps_f64()).round() as i64
    }

    /// Convert an integer frame count back to real-world seconds.
    pub fn frames_to_seconds(&self, frames: i64) -> f64 {
        frames as f64 / self.fps_f64()
    }

    /// Convert an absolute frame counter (0 = 00:00:00:00) into SMPTE HH:MM:SS:FF / HH:MM:SS;FF.
    pub fn format_frames(&self, frame_index: u64) -> String {
        let (hh, mm, ss, ff) = self.frames_to_hmsf(frame_index);
        let sep = self.frame_separator();
        format!("{hh:02}:{mm:02}:{ss:02}{sep}{ff:02}")
    }

    pub fn frames_to_hmsf(&self, frame_index: u64) -> (u64, u64, u64, u64) {
        let nom = self.nominal_fps();
        let adjusted = if self.is_drop_frame() {
            let drop = if nom == 30 { 2u64 } else { 4u64 };
            let frames_per_10min = nom * 600 - drop * 9; // 17982 or 35964
            let frames_per_min = nom * 60 - drop; // 1798 or 3596
            let d = frame_index / frames_per_10min;
            let m = frame_index % frames_per_10min;
            let extra = if m >= drop {
                drop * ((m - drop) / frames_per_min)
            } else {
                0
            };
            frame_index + drop * 9 * d + extra
        } else {
            frame_index
        };

        let ff = adjusted % nom;
        let total_secs = adjusted / nom;
        let ss = total_secs % 60;
        let total_mins = total_secs / 60;
        let mm = total_mins % 60;
        let hh = total_mins / 60;
        (hh, mm, ss, ff)
    }

    /// Parse SMPTE timecode `HH:MM:SS:FF` or `HH:MM:SS;FF` into an absolute frame counter.
    pub fn parse_smpte(&self, tc: &str) -> Result<u64, TimecodeError> {
        let trimmed = tc.trim();
        let parts: Vec<&str> = trimmed.split([':', ';']).collect();
        if parts.len() != 4 {
            return Err(TimecodeError::InvalidFormat(tc.to_string()));
        }
        let hh: u64 = parts[0]
            .parse()
            .map_err(|_| TimecodeError::InvalidFormat(tc.to_string()))?;
        let mm: u64 = parts[1]
            .parse()
            .map_err(|_| TimecodeError::InvalidFormat(tc.to_string()))?;
        let ss: u64 = parts[2]
            .parse()
            .map_err(|_| TimecodeError::InvalidFormat(tc.to_string()))?;
        let ff: u64 = parts[3]
            .parse()
            .map_err(|_| TimecodeError::InvalidFormat(tc.to_string()))?;

        let nom = self.nominal_fps();
        if hh >= 24 || mm >= 60 || ss >= 60 || ff >= nom {
            return Err(TimecodeError::OutOfRange(tc.to_string()));
        }

        let total_minutes = hh * 60 + mm;
        if self.is_drop_frame() {
            let drop = if nom == 30 { 2u64 } else { 4u64 };
            // Reject nonexistent dropped frame numbers at non-10th minute boundaries
            if ss == 0 && (mm % 10 != 0) && ff < drop {
                return Err(TimecodeError::OutOfRange(tc.to_string()));
            }
            let dropped = drop * (total_minutes - total_minutes / 10);
            let nominal_frames = ((hh * 3600 + mm * 60 + ss) * nom) + ff;
            Ok(nominal_frames - dropped)
        } else {
            Ok(((hh * 3600 + mm * 60 + ss) * nom) + ff)
        }
    }

    /// Format a frame count as an exact FCPXML rational time string (e.g. `"90000/25s"` or `"90090/24000s"`).
    pub fn frames_to_fcpxml_time(&self, frame_index: u64) -> String {
        let (num, den) = self.rational();
        let ticks = frame_index.saturating_mul(den);
        format!("{ticks}/{num}s")
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_25fps_and_one_hour_roundtrips() {
        let fps = FrameRate::default();
        assert_eq!(fps, FrameRate::Fps25);
        let start = fps.parse_smpte("01:00:00:00").unwrap();
        assert_eq!(start, 90_000);
        assert_eq!(fps.format_frames(start), "01:00:00:00");
        assert_eq!(fps.format_frames(start + 25), "01:00:01:00");
    }

    #[test]
    fn drop_frame_2997_minute_boundaries() {
        let fps = FrameRate::Fps29_97Df;
        // Minute 0 has all 1800 frames (0..=1799): Frame 1799 = 00:00:59;29 -> Frame 1800 = 00:01:00;02
        assert_eq!(fps.format_frames(1799), "00:00:59;29");
        assert_eq!(fps.format_frames(1800), "00:01:00;02");
        assert_eq!(fps.parse_smpte("00:01:00;02").unwrap(), 1800);
        assert!(fps.parse_smpte("00:01:00;00").is_err());
        // Minute 1 has 1798 frames (1800..=3597): Frame 3597 = 00:01:59;29 -> Frame 3598 = 00:02:00;02
        assert_eq!(fps.format_frames(3597), "00:01:59;29");
        assert_eq!(fps.format_frames(3598), "00:02:00;02");
        // 10th minute does NOT drop frames: 17981 = 00:09:59;29 -> 17982 = 00:10:00;00
        assert_eq!(fps.format_frames(17981), "00:09:59;29");
        assert_eq!(fps.format_frames(17982), "00:10:00;00");
        assert_eq!(fps.parse_smpte("00:10:00;00").unwrap(), 17982);
        // 1 hour DF = 107892 frames
        let one_hr = fps.parse_smpte("01:00:00;00").unwrap();
        assert_eq!(one_hr, 107_892);
        assert_eq!(fps.format_frames(one_hr), "01:00:00;00");
    }

    #[test]
    fn drop_frame_5994_minute_boundaries() {
        let fps = FrameRate::Fps59_94Df;
        // Minute 0 has all 3600 frames (0..=3599): Frame 3599 = 00:00:59;59 -> Frame 3600 = 00:01:00;04
        assert_eq!(fps.format_frames(3599), "00:00:59;59");
        assert_eq!(fps.format_frames(3600), "00:01:00;04");
        assert_eq!(fps.parse_smpte("00:01:00;04").unwrap(), 3600);
        assert_eq!(fps.format_frames(35963), "00:09:59;59");
        assert_eq!(fps.format_frames(35964), "00:10:00;00");
    }

    #[test]
    fn pulldown_23976_no_drift_at_1000s() {
        let fps = FrameRate::Fps23_976;
        // 1000 real seconds at 24000/1001 = 23976.023976 frames -> 23976 frames
        let frames = fps.seconds_to_frames(1000.0);
        assert_eq!(frames, 23_976);
        // 23976 frames at 24 nominal fps = 999 seconds = 00:16:39:00
        assert_eq!(fps.format_frames(frames as u64), "00:16:39:00");
    }

    #[test]
    fn smpte_rejects_hours_ge_24() {
        let fps = FrameRate::Fps25;
        assert!(fps.parse_smpte("23:59:59:24").is_ok());
        assert!(fps.parse_smpte("24:00:00:00").is_err());
        assert!(fps.parse_smpte("99:00:00:00").is_err());
    }
}
