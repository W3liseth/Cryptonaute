//! Formatage des valeurs affichées (octets, débits, durées), à la française.

/// Nombre avec au plus une décimale, virgule décimale (`Intl.NumberFormat('fr-FR')`).
fn one_decimal(n: f64) -> String {
    let rounded = (n * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}").replace('.', ",")
    }
}

/// 1 234 567 → « 1,2 Mo ».
pub fn bytes(n: f64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut n = n.max(0.0);
    let mut i = 0;
    while n >= 1000.0 && i < UNITS.len() - 1 {
        n /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n.round(), UNITS[0])
    } else {
        format!("{} {}", one_decimal(n), UNITS[i])
    }
}

pub fn rate(n: f64) -> String {
    format!("{}/s", bytes(n))
}

/// Secondes → « 01:02:03 ».
pub fn duration(secs: f64) -> String {
    let s = secs.max(0.0).floor() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Ancienneté d'une poignée de main.
pub fn ago(secs: u64) -> String {
    match secs {
        0..=4 => "à l'instant".into(),
        5..=59 => format!("il y a {secs} s"),
        60..=3599 => format!("il y a {} min", secs / 60),
        _ => format!("il y a {} h", secs / 3600),
    }
}

/// Teinte stable dérivée du nom (pastille de la liste), comme l'ancienne interface.
pub fn hue(name: &str) -> f32 {
    name.encode_utf16().fold(7u32, |h, c| (h * 31 + c as u32) % 360) as f32
}

/// Date ISO « 2026-10-09 » → « 9 octobre 2026 ».
pub fn date_fr(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
        "novembre", "décembre",
    ];
    let mut parts = iso.split('-');
    let (y, m, d) = (parts.next()?, parts.next()?.parse::<usize>().ok()?, parts.next()?.parse::<u32>().ok()?);
    Some(format!("{d} {} {y}", MONTHS.get(m.checked_sub(1)?)?))
}

pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_formats() {
        assert_eq!(bytes(0.0), "0 o");
        assert_eq!(bytes(999.0), "999 o");
        assert_eq!(bytes(614_700.0), "614,7 Ko");
        assert_eq!(bytes(6_000_000.0), "6 Mo");
        assert_eq!(rate(1_100.0), "1,1 Ko/s");
        assert_eq!(duration(3723.9), "01:02:03");
        assert_eq!(ago(12), "il y a 12 s");
        assert_eq!(date_fr("2026-10-09").unwrap(), "9 octobre 2026");
        // Même teinte que l'ancienne interface JavaScript.
        assert_eq!(hue("a"), 314.0); // (7 × 31 + 97) mod 360
    }
}
