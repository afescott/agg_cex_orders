/// Parse a decimal price string into cents (2 decimal places).
/// Returns `None` if the string cannot be parsed.
pub fn parse_price_cents(s: &str) -> Option<u64> {
    let mut parts = s.split('.');
    let int_part = parts.next()?;
    let frac_part = parts.next();

    // More than one '.' is considered invalid
    if parts.next().is_some() {
        return None;
    }

    let int_val: u64 = int_part.parse().ok()?;

    let frac_val = if let Some(frac) = frac_part {
        let mut frac = frac.to_string();
        // We only care about 2 decimal places for "cents"
        if frac.len() > 2 {
            frac.truncate(2);
        } else {
            while frac.len() < 2 {
                frac.push('0');
            }
        }
        frac.parse::<u64>().ok()?
    } else {
        0
    };

    int_val
        .checked_mul(100)?
        .checked_add(frac_val)
}

/// Parse a decimal quantity string into the smallest unit given by `decimals`.
/// For example, with `decimals = 8`, "0.00000001" becomes 1.
pub fn parse_quantity_smallest_unit(s: &str, decimals: u32) -> Option<u64> {
    let mut parts = s.split('.');
    let int_part = parts.next()?;
    let frac_part = parts.next();

    // More than one '.' is considered invalid
    if parts.next().is_some() {
        return None;
    }

    let int_val: u64 = int_part.parse().ok()?;

    let scale = 10u64.checked_pow(decimals)?;

    let frac_val = if let Some(frac) = frac_part {
        let mut frac = frac.to_string();
        if frac.len() > decimals as usize {
            frac.truncate(decimals as usize);
        } else {
            while frac.len() < decimals as usize {
                frac.push('0');
            }
        }
        frac.parse::<u64>().ok()?
    } else {
        0
    };

    int_val
        .checked_mul(scale)?
        .checked_add(frac_val)
}

