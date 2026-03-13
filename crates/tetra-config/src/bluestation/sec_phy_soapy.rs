use serde::Deserialize;
use std::collections::HashMap;
use toml::Value;

/// Configuration for different SDR hardware devices
#[derive(Debug, Clone)]
pub struct SoapySdrIoCfg {
    /// USRP B2xx series configuration (B200, B210)
    pub iocfg_usrpb2xx: Option<CfgUsrpB2xx>,

    /// LimeSDR configuration
    pub iocfg_limesdr: Option<CfgLimeSdr>,

    /// SXceiver configuration
    pub iocfg_sxceiver: Option<CfgSxCeiver>,
}

impl SoapySdrIoCfg {
    pub fn get_soapy_driver_name(&self) -> &'static str {
        if self.iocfg_usrpb2xx.is_some() {
            "uhd"
        } else if self.iocfg_limesdr.is_some() {
            "lime"
        } else if self.iocfg_sxceiver.is_some() {
            "sx"
        } else {
            "unknown"
        }
    }
}

impl Default for SoapySdrIoCfg {
    fn default() -> Self {
        Self {
            iocfg_usrpb2xx: None,
            iocfg_limesdr: None,
            iocfg_sxceiver: None,
        }
    }
}

/// Configuration for Ettus USRP B2xx series
#[derive(Debug, Clone, Deserialize)]
pub struct CfgUsrpB2xx {
    pub rx_ant: Option<String>,
    pub tx_ant: Option<String>,
    pub rx_gain_pga: Option<f64>,
    pub tx_gain_pga: Option<f64>,
}

/// Configuration for LimeSDR
#[derive(Debug, Clone, Deserialize)]
pub struct CfgLimeSdr {
    pub rx_ant: Option<String>,
    pub tx_ant: Option<String>,
    pub rx_gain_lna: Option<f64>,
    pub rx_gain_tia: Option<f64>,
    pub rx_gain_pga: Option<f64>,
    pub tx_gain_pad: Option<f64>,
    pub tx_gain_iamp: Option<f64>,
}

/// Configuration for SXceiver
#[derive(Debug, Clone, Deserialize)]
pub struct CfgSxCeiver {
    pub rx_ant: Option<String>,
    pub tx_ant: Option<String>,
    pub rx_gain_lna: Option<f64>,
    pub rx_gain_pga: Option<f64>,
    pub tx_gain_dac: Option<f64>,
    pub tx_gain_mixer: Option<f64>,
}

/// Polynomial pre-distortion configuration for PA linearisation.
///
/// Applies a memoryless complex polynomial correction to each complex baseband
/// sample immediately before it is handed to the SDR, operating on |x|²
/// (squared magnitude) to avoid the sqrt in a hot path:
///
///   y[n] = x[n] * P(|x[n]|²)
///
/// where P is a complex polynomial:
///
///   P(u) = (a0 + a1·u + a2·u² + ...) + j·(b0 + b1·u + b2·u² + ...)
///        = A(u) + j·B(u)
///
/// Both polynomials are evaluated via Horner's method.
///
/// - **A(u)** corrects AM/AM distortion (magnitude compression/expansion).
/// - **B(u)** corrects AM/PM distortion (phase rotation as a function of drive level).
///
/// The combined complex gain rotates each phasor by a drive-level-dependent
/// angle while also scaling it, linearising both the amplitude and phase
/// responses of the PA simultaneously.
///
/// # Identity / pass-through
///
/// `am_am_coefficients = [1.0]`, `am_pm_coefficients = [0.0]`
/// (or omit the section entirely) → y = x · (1 + j·0) = x.
///
/// # Coefficient guide
///
/// AM/AM (`am_am_coefficients`, real part of P):
/// ```
///   [1.0]                  — unity gain, no AM/AM correction
///   [1.0, 0.0, -0.15, 0.05]  — typical mild compression correction
/// ```
/// The first coefficient (c₀) is the small-signal gain; subsequent terms
/// progressively reduce gain at higher drive levels to pre-expand the signal
/// before the compressive PA.
///
/// AM/PM (`am_pm_coefficients`, imaginary part of P):
/// ```
///   [0.0]                  — no AM/PM correction (phase preserved)
///   [0.0, 0.05]            — linear phase tilt with drive level
/// ```
/// A positive coefficient introduces a positive (leading) phase pre-rotation
/// that compensates for a PA that lags phase with increasing drive.
///
/// # Fitting procedure
///
/// 1. Drive the PA at several power levels and measure output amplitude and
///    phase relative to a linear reference (VNA, spectrum analyser + coupler,
///    or the SDR's own observation path if available).
/// 2. Build the AM/AM inverse curve (desired_output / actual_output vs |x|²).
/// 3. Build the AM/PM inverse curve (−measured_phase_shift vs |x|²).
/// 4. Fit polynomials to both inverse curves (e.g. with NumPy polyfit).
/// 5. Enter the coefficients here.
///
/// Omit the `[phy_io.soapysdr.predistortion]` section entirely to disable DPD
/// with zero runtime overhead.
#[derive(Debug, Clone, Deserialize)]
pub struct CfgPredistortion {
    /// Real part of the complex DPD polynomial P(|x|²).
    /// Corrects AM/AM (amplitude) distortion.
    /// Must contain at least one element.  [1.0] = unity (no AM/AM correction).
    pub am_am_coefficients: Vec<f32>,

    /// Imaginary part of the complex DPD polynomial P(|x|²).
    /// Corrects AM/PM (phase) distortion.
    /// Must contain at least one element.  [0.0] = no phase correction.
    #[serde(default = "default_am_pm_coefficients")]
    pub am_pm_coefficients: Vec<f32>,
}

fn default_am_pm_coefficients() -> Vec<f32> {
    vec![0.0]
}

impl CfgPredistortion {
    /// Returns true when the polynomial is the identity transform,
    /// i.e. P(u) = 1 + j·0 for all u, so the fast path can skip DPD entirely.
    pub fn is_identity(&self) -> bool {
        let am_am_identity = self.am_am_coefficients.len() == 1 && self.am_am_coefficients[0] == 1.0;
        let am_pm_identity = self.am_pm_coefficients.len() == 1 && self.am_pm_coefficients[0] == 0.0;
        am_am_identity && am_pm_identity
    }
}

impl Default for CfgPredistortion {
    fn default() -> Self {
        // Identity: y = x · (1 + j·0) = x
        Self {
            am_am_coefficients: vec![1.0],
            am_pm_coefficients: vec![0.0],
        }
    }
}

/// SoapySDR configuration
#[derive(Debug, Clone)]
pub struct CfgSoapySdr {
    /// Uplink frequency in Hz
    pub ul_freq: f64,
    /// Downlink frequency in Hz
    pub dl_freq: f64,
    /// PPM frequency error correction
    pub ppm_err: f64,
    /// Hardware-specific I/O configuration
    pub io_cfg: SoapySdrIoCfg,
    /// Optional polynomial pre-distortion for PA linearisation.
    /// None means DPD is disabled (samples pass through unmodified).
    pub predistortion: Option<CfgPredistortion>,
}

impl CfgSoapySdr {
    /// Get corrected UL frequency with PPM error applied
    pub fn ul_freq_corrected(&self) -> (f64, f64) {
        let ppm = self.ppm_err;
        let err = (self.ul_freq / 1_000_000.0) * ppm;
        (self.ul_freq + err, err)
    }

    /// Get corrected DL frequency with PPM error applied
    pub fn dl_freq_corrected(&self) -> (f64, f64) {
        let ppm = self.ppm_err;
        let err = (self.dl_freq / 1_000_000.0) * ppm;
        (self.dl_freq + err, err)
    }
}

/// DTO for deserialising the `[phy_io.soapysdr.predistortion]` TOML section.
/// Mirrors `CfgPredistortion` but uses serde defaults so that omitting
/// `am_pm_coefficients` is legal (defaults to [0.0]).
#[derive(Deserialize)]
pub struct PredistortionDto {
    pub am_am_coefficients: Vec<f32>,
    #[serde(default = "default_am_pm_coefficients")]
    pub am_pm_coefficients: Vec<f32>,
}

impl From<PredistortionDto> for CfgPredistortion {
    fn from(dto: PredistortionDto) -> Self {
        Self {
            am_am_coefficients: dto.am_am_coefficients,
            am_pm_coefficients: dto.am_pm_coefficients,
        }
    }
}

#[derive(Deserialize)]
pub struct SoapySdrDto {
    pub rx_freq: f64,
    pub tx_freq: f64,
    pub ppm_err: Option<f64>,

    pub iocfg_usrpb2xx: Option<UsrpB2xxDto>,
    pub iocfg_limesdr: Option<LimeSdrDto>,
    pub iocfg_sxceiver: Option<SXceiverDto>,

    /// Optional complex polynomial pre-distortion section.
    pub predistortion: Option<PredistortionDto>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Deserialize)]
pub struct UsrpB2xxDto {
    pub rx_ant: Option<String>,
    pub tx_ant: Option<String>,
    pub rx_gain_pga: Option<f64>,
    pub tx_gain_pga: Option<f64>,
}

#[derive(Deserialize)]
pub struct LimeSdrDto {
    pub rx_ant: Option<String>,
    pub tx_ant: Option<String>,
    pub rx_gain_lna: Option<f64>,
    pub rx_gain_tia: Option<f64>,
    pub rx_gain_pga: Option<f64>,
    pub tx_gain_pad: Option<f64>,
    pub tx_gain_iamp: Option<f64>,
}

#[derive(Deserialize)]
pub struct SXceiverDto {
    pub rx_ant: Option<String>,
    pub tx_ant: Option<String>,
    pub rx_gain_lna: Option<f64>,
    pub rx_gain_pga: Option<f64>,
    pub tx_gain_dac: Option<f64>,
    pub tx_gain_mixer: Option<f64>,
}
