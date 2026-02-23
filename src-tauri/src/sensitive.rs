use fancy_regex::Regex;
use std::sync::LazyLock;

struct Pattern {
    re: Regex,
    validate: Option<fn(&str) -> bool>,
}

impl Pattern {
    fn new(pat: &str) -> Self {
        Self { re: Regex::new(pat).unwrap(), validate: None }
    }
    fn with_validator(pat: &str, v: fn(&str) -> bool) -> Self {
        Self { re: Regex::new(pat).unwrap(), validate: Some(v) }
    }
    fn matches(&self, text: &str) -> bool {
        let mut start = 0;
        while start < text.len() {
            match self.re.find_from_pos(text, start) {
                Ok(Some(m)) => {
                    if let Some(v) = self.validate {
                        if v(m.as_str()) { return true; }
                    } else {
                        return true;
                    }
                    start = m.end().max(start + 1);
                }
                _ => break,
            }
        }
        false
    }
}

// ── Validators ──

fn luhn_check(raw: &str) -> bool {
    let digits: Vec<u32> = raw.chars().filter(|c| c.is_ascii_digit()).collect::<Vec<_>>()
        .iter().map(|c| c.to_digit(10).unwrap()).collect();
    if digits.len() < 13 || digits.len() > 19 { return false; }
    let mut sum = 0u32;
    let mut double = false;
    for &d in digits.iter().rev() {
        let mut n = d;
        if double { n *= 2; if n > 9 { n -= 9; } }
        sum += n;
        double = !double;
    }
    sum % 10 == 0
}

fn china_id_check(raw: &str) -> bool {
    let digits: Vec<char> = raw.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if digits.len() != 18 { return false; }
    let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let check_chars = ['1', '0', 'X', '9', '8', '7', '6', '5', '4', '3', '2'];
    let mut sum = 0usize;
    for i in 0..17 {
        if let Some(d) = digits[i].to_digit(10) {
            sum += d as usize * weights[i];
        } else {
            return false;
        }
    }
    let expected = check_chars[sum % 11];
    digits[17].to_ascii_uppercase() == expected
}

// ── Universal patterns (all regions) ──

static UNIVERSAL: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Email
    Pattern::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b"),
    // Credit/debit card (with Luhn)
    Pattern::with_validator(
        r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{3,4}\b",
        luhn_check,
    ),
    // IPv4
    Pattern::new(r"\b(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b"),
    // AWS Access Key
    Pattern::new(r"\bAKIA[0-9A-Z]{16}\b"),
    // Generic API key / secret patterns
    Pattern::new(r"(?i)\b(?:sk|pk)_(?:live|test)_[a-z0-9]{20,}\b"),
    // JWT
    Pattern::new(r"\beyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_.+/=]+\b"),
    // IBAN (international bank account)
    Pattern::new(r"\b[A-Z]{2}\d{2}[\s]?[A-Z0-9]{4}[\s]?(?:[A-Z0-9]{4}[\s]?){2,7}[A-Z0-9]{1,4}\b"),
]);

// Password / secret keywords (checked separately, case-insensitive substring)
static KEYWORDS: &[&str] = &[
    "password", "passwd", "密码", "密碼", "secret", "token",
    "api_key", "apikey", "api-key", "access_key", "private_key",
    "client_secret", "パスワード", "비밀번호", "пароль", "كلمة المرور",
    "mật khẩu", "รหัสผ่าน", "kata sandi", "şifre", "hasło", "wachtwoord",
    "passwort", "contraseña", "senha", "mot de passe",
];

// ── Regional patterns ──

// China (zh-CN)
static CN: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone
    Pattern::new(r"(?<!\d)1[3-9]\d{9}(?!\d)"),
    // ID card (18 digits with checksum)
    Pattern::with_validator(r"(?<!\d)\d{17}[\dXx](?!\d)", china_id_check),
]);

// Taiwan (zh-TW)
static TW: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    Pattern::new(r"(?<!\d)09\d{8}(?!\d)"),
    // National ID: letter + [12] + 8 digits
    Pattern::new(r"(?<![A-Za-z])[A-Z][12]\d{8}(?!\d)"),
]);

// English (US + UK)
static EN: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // US phone: (xxx) xxx-xxxx or xxx-xxx-xxxx
    Pattern::new(r"(?<!\d)\(?\d{3}\)?[\s\-\.]\d{3}[\s\-\.]\d{4}(?!\d)"),
    // US SSN
    Pattern::new(r"(?<!\d)\d{3}\-\d{2}\-\d{4}(?!\d)"),
    // UK NINO (National Insurance)
    Pattern::new(r"(?i)\b[A-CEGHJ-PR-TW-Z][A-CEGHJ-NPR-TW-Z]\s?\d{2}\s?\d{2}\s?\d{2}\s?[A-D]\b"),
    // UK phone
    Pattern::new(r"(?<!\d)(?:\+44[\s\-]?|0)7\d{3}[\s\-]?\d{6}(?!\d)"),
]);

// Japanese (ja)
static JA: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 0[789]0-XXXX-XXXX
    Pattern::new(r"(?<!\d)0[789]0[\-\s]?\d{4}[\-\s]?\d{4}(?!\d)"),
    // My Number (12 digits)
    Pattern::new(r"(?<!\d)\d{4}[\s]?\d{4}[\s]?\d{4}(?!\d)"),
]);

// Korean (ko)
static KO: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 01X-XXXX-XXXX
    Pattern::new(r"(?<!\d)01[016789][\-\s]?\d{3,4}[\-\s]?\d{4}(?!\d)"),
    // Resident Registration Number (6-7 digits)
    Pattern::new(r"(?<!\d)\d{6}[\-\s]\d{7}(?!\d)"),
]);

// French (fr)
static FR: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 06/07 XX XX XX XX
    Pattern::new(r"(?<!\d)(?:\+33[\s\-]?|0)[67](?:[\s\.\-]?\d{2}){4}(?!\d)"),
    // INSEE / Social Security (15 digits)
    Pattern::new(r"(?<!\d)[12]\s?\d{2}\s?\d{2}\s?\d{2}\s?\d{3}\s?\d{3}\s?\d{2}(?!\d)"),
]);

// German (de)
static DE: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 015x/016x/017x
    Pattern::new(r"(?<!\d)(?:\+49[\s\-]?|0)1[567]\d[\s\-]?\d{3,4}[\s\-]?\d{4}(?!\d)"),
    // Tax ID (Steuerliche Identifikationsnummer, 11 digits)
    Pattern::new(r"(?<!\d)\d{11}(?!\d)"),
]);

// Spanish (es)
static ES: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 6XX or 7XX
    Pattern::new(r"(?<!\d)(?:\+34[\s\-]?)?[67]\d{2}[\s\-]?\d{3}[\s\-]?\d{3}(?!\d)"),
    // DNI: 8 digits + letter
    Pattern::new(r"(?<!\d)\d{8}[\-\s]?[A-Z](?![A-Za-z])"),
    // NIE: X/Y/Z + 7 digits + letter
    Pattern::new(r"(?<![A-Za-z])[XYZ]\d{7}[\-\s]?[A-Z](?![A-Za-z])"),
]);

// Portuguese (pt - Brazil + Portugal)
static PT: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Brazil CPF: XXX.XXX.XXX-XX
    Pattern::new(r"(?<!\d)\d{3}\.?\d{3}\.?\d{3}[\-]?\d{2}(?!\d)"),
    // Brazil phone: (XX) 9XXXX-XXXX
    Pattern::new(r"(?<!\d)\(?\d{2}\)?[\s\-]?9\d{4}[\-\s]?\d{4}(?!\d)"),
    // Portugal phone: 9X
    Pattern::new(r"(?<!\d)(?:\+351[\s\-]?)?9[1236]\d[\s\-]?\d{3}[\s\-]?\d{3}(?!\d)"),
]);

// Russian (ru)
static RU: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: +7 9XX XXX-XX-XX
    Pattern::new(r"(?<!\d)(?:\+7|8)[\s\-]?9\d{2}[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}(?!\d)"),
    // Passport: XXXX XXXXXX
    Pattern::new(r"(?<!\d)\d{4}[\s]\d{6}(?!\d)"),
    // SNILS: XXX-XXX-XXX XX
    Pattern::new(r"(?<!\d)\d{3}[\-]\d{3}[\-]\d{3}[\s]\d{2}(?!\d)"),
]);

// Arabic (ar - Saudi, Egypt, UAE)
static AR: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Saudi mobile: 05X XXXX XXX
    Pattern::new(r"(?<!\d)(?:\+966[\s\-]?)?05\d[\s\-]?\d{3}[\s\-]?\d{4}(?!\d)"),
    // Egypt mobile: 01[0125] XXXX XXXX
    Pattern::new(r"(?<!\d)(?:\+20[\s\-]?)?01[0125]\d{8}(?!\d)"),
    // UAE mobile: 05X XXX XXXX
    Pattern::new(r"(?<!\d)(?:\+971[\s\-]?)?05[0-9]\d[\s\-]?\d{3}[\s\-]?\d{4}(?!\d)"),
    // Saudi national ID (10 digits starting with 1 or 2)
    Pattern::new(r"(?<!\d)[12]\d{9}(?!\d)"),
]);

// Thai (th)
static TH: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 06/08/09
    Pattern::new(r"(?<!\d)(?:\+66[\s\-]?)?0[689]\d[\s\-]?\d{3}[\s\-]?\d{4}(?!\d)"),
    // National ID (13 digits)
    Pattern::new(r"(?<!\d)\d[\-\s]?\d{4}[\-\s]?\d{5}[\-\s]?\d{2}[\-\s]?\d(?!\d)"),
]);

// Vietnamese (vi)
static VI: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 0[35789]X
    Pattern::new(r"(?<!\d)(?:\+84[\s\-]?)?0[35789]\d[\s\-]?\d{3}[\s\-]?\d{3}(?!\d)"),
    // New ID (12 digits)
    Pattern::new(r"(?<!\d)0\d{2}\d{9}(?!\d)"),
]);

// Italian (it)
static IT: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 3XX
    Pattern::new(r"(?<!\d)(?:\+39[\s\-]?)?3\d{2}[\s\-]?\d{3}[\s\-]?\d{4}(?!\d)"),
    // Codice Fiscale (16 alphanumeric)
    Pattern::new(r"(?<![A-Za-z])[A-Z]{6}\d{2}[A-Z]\d{2}[A-Z]\d{3}[A-Z](?![A-Za-z])"),
]);

// Dutch (nl)
static NL: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 06
    Pattern::new(r"(?<!\d)(?:\+31[\s\-]?|0)6[\s\-]?\d{2}[\s\-]?\d{2}[\s\-]?\d{2}[\s\-]?\d{2}(?!\d)"),
    // BSN (Burgerservicenummer, 9 digits)
    Pattern::new(r"(?<!\d)\d{9}(?!\d)"),
]);

// Polish (pl)
static PL: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: [4-9]XX XXX XXX
    Pattern::new(r"(?<!\d)(?:\+48[\s\-]?)?[4-9]\d{2}[\s\-]?\d{3}[\s\-]?\d{3}(?!\d)"),
    // PESEL (11 digits)
    Pattern::new(r"(?<!\d)\d{11}(?!\d)"),
]);

// Turkish (tr)
static TR: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 5XX
    Pattern::new(r"(?<!\d)(?:\+90[\s\-]?)?5\d{2}[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}(?!\d)"),
    // TC Kimlik (11 digits, starts with non-zero)
    Pattern::new(r"(?<!\d)[1-9]\d{10}(?!\d)"),
]);

// Ukrainian (uk)
static UK: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 0[3-9]X
    Pattern::new(r"(?<!\d)(?:\+380[\s\-]?|0)[3-9]\d[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}(?!\d)"),
    // INN (РНОКПП, 10 digits)
    Pattern::new(r"(?<!\d)\d{10}(?!\d)"),
]);

// Indonesian (id)
static ID: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: 08XX
    Pattern::new(r"(?<!\d)(?:\+62[\s\-]?|0)8\d{2}[\s\-]?\d{4}[\s\-]?\d{3,4}(?!\d)"),
    // NIK (16 digits)
    Pattern::new(r"(?<!\d)\d{16}(?!\d)"),
]);

// Hindi / India (hi)
static HI: LazyLock<Vec<Pattern>> = LazyLock::new(|| vec![
    // Mobile phone: [6-9]XXXXXXXXX
    Pattern::new(r"(?<!\d)(?:\+91[\s\-]?)?[6-9]\d{4}[\s\-]?\d{5}(?!\d)"),
    // Aadhaar (12 digits in groups of 4)
    Pattern::new(r"(?<!\d)\d{4}[\s\-]?\d{4}[\s\-]?\d{4}(?!\d)"),
    // PAN card: ABCDE1234F
    Pattern::new(r"(?<![A-Za-z])[A-Z]{5}\d{4}[A-Z](?![A-Za-z])"),
]);

fn get_regional_patterns(lang: &str) -> &'static [Pattern] {
    match lang {
        "zh-CN" => &*CN,
        "zh-TW" => &*TW,
        "en" => &*EN,
        "ja" => &*JA,
        "ko" => &*KO,
        "fr" => &*FR,
        "de" => &*DE,
        "es" => &*ES,
        "pt" => &*PT,
        "ru" => &*RU,
        "ar" => &*AR,
        "th" => &*TH,
        "vi" => &*VI,
        "it" => &*IT,
        "nl" => &*NL,
        "pl" => &*PL,
        "tr" => &*TR,
        "uk" => &*UK,
        "id" => &*ID,
        "hi" => &*HI,
        _ => &*EN, // fallback
    }
}

pub fn detect_sensitive(text: &str, language: &str) -> bool {
    if text.len() < 6 { return false; }

    // Keyword check (fast path)
    let lower = text.to_lowercase();
    for kw in KEYWORDS {
        if lower.contains(kw) { return true; }
    }

    // Universal patterns
    for pat in UNIVERSAL.iter() {
        if pat.matches(text) { return true; }
    }

    // Regional patterns
    for pat in get_regional_patterns(language) {
        if pat.matches(text) { return true; }
    }

    false
}
