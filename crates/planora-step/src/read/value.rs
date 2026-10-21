//! Werte und Einträge einer STEP-Datei, wie `StepFile::entity` sie liefert. Typnamen stehen in
//! Großbuchstaben. Die Zugriffe melden Fehler als `String` (Grund ohne Zeile); der Aufrufer
//! setzt die Zeile davor.

/// Ein Parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    Str(String),
    /// Aufzählung ohne Punkte, in Großbuchstaben (`.T.` → `"T"`).
    Enum(String),
    Ref(u64),
    List(Vec<Value>),
    /// Getypter Wert wie `LENGTH_MEASURE(1.0)`.
    Typed(String, Vec<Value>),
    /// `$`
    Unset,
    /// `*`
    Derived,
}

impl Value {
    /// Kurze Beschreibung für Meldungen.
    fn kind(&self) -> &'static str {
        match self {
            Value::Int(_) => "eine ganze Zahl",
            Value::Real(_) => "eine Zahl",
            Value::Str(_) => "ein Text",
            Value::Enum(_) => "eine Aufzählung",
            Value::Ref(_) => "ein Verweis",
            Value::List(_) => "eine Liste",
            Value::Typed(..) => "ein getypter Wert",
            Value::Unset => "kein Wert („$“)",
            Value::Derived => "ein abgeleiteter Wert („*“)",
        }
    }

    fn wrong(&self, want: &str) -> String {
        format!("Erwartet wird {want}, gefunden: {}.", self.kind())
    }

    /// Zahl; nimmt auch ganze Zahlen (nachsichtig).
    pub fn real(&self) -> Result<f64, String> {
        match self {
            Value::Real(x) => Ok(*x),
            Value::Int(i) => Ok(*i as f64),
            other => Err(other.wrong("eine Zahl")),
        }
    }

    /// Zahl, auch als getypter Wert mit genau einer Zahl (`LENGTH_MEASURE(1.E-3)`,
    /// `PLANE_ANGLE_MEASURE(0.01745)`); den Typnamen prüft der Aufrufer bei Bedarf selbst.
    pub fn measure(&self) -> Result<f64, String> {
        match self {
            Value::Typed(_, args) if args.len() == 1 => args[0].real(),
            Value::Typed(name, _) => Err(format!("Erwartet wird {name} mit genau einer Zahl.")),
            other => other.real(),
        }
    }

    pub fn int(&self) -> Result<i64, String> {
        match self {
            Value::Int(i) => Ok(*i),
            other => Err(other.wrong("eine ganze Zahl")),
        }
    }

    pub fn reference(&self) -> Result<u64, String> {
        match self {
            Value::Ref(r) => Ok(*r),
            other => Err(other.wrong("ein Verweis")),
        }
    }

    pub fn list(&self) -> Result<&[Value], String> {
        match self {
            Value::List(items) => Ok(items),
            other => Err(other.wrong("eine Liste")),
        }
    }

    pub fn text(&self) -> Result<&str, String> {
        match self {
            Value::Str(s) => Ok(s),
            other => Err(other.wrong("ein Text")),
        }
    }

    pub fn enumeration(&self) -> Result<&str, String> {
        match self {
            Value::Enum(e) => Ok(e),
            other => Err(other.wrong("eine Aufzählung")),
        }
    }

    /// `.T.` → `Some(true)`, `.F.` → `Some(false)`, `.U.` → `None`.
    pub fn logical(&self) -> Result<Option<bool>, String> {
        match self.enumeration()? {
            "T" | "TRUE" => Ok(Some(true)),
            "F" | "FALSE" => Ok(Some(false)),
            "U" | "UNKNOWN" => Ok(None),
            other => Err(format!(
                "Erwartet wird .T., .F. oder .U., gefunden: .{other}."
            )),
        }
    }

    /// Liste von Verweisen.
    pub fn refs(&self) -> Result<Vec<u64>, String> {
        self.list()?.iter().map(Value::reference).collect()
    }

    /// Liste von Zahlen (ganze Zahlen werden genommen).
    pub fn reals(&self) -> Result<Vec<f64>, String> {
        self.list()?.iter().map(Value::real).collect()
    }

    /// `$` oder `*`.
    pub fn is_unset(&self) -> bool {
        matches!(self, Value::Unset | Value::Derived)
    }
}

/// Ein Typ mit seinen Parametern.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub name: String,
    pub args: Vec<Value>,
}

/// Ein Eintrag: einfach `#n = TYP(…);` oder zusammengesetzt `#n = (A(…) B(…));`.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Simple(Record),
    Complex(Vec<Record>),
}

impl Entity {
    fn records(&self) -> &[Record] {
        match self {
            Entity::Simple(r) => std::slice::from_ref(r),
            Entity::Complex(parts) => parts,
        }
    }

    /// Einfacher Eintrag dieses Typs oder zusammengesetzter mit diesem Teil.
    pub fn is(&self, name: &str) -> bool {
        self.part(name).is_some()
    }

    /// Parameter des Teils `name` (auch für einfache Einträge dieses Typs).
    pub fn part(&self, name: &str) -> Option<&[Value]> {
        self.records()
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .map(|r| r.args.as_slice())
    }

    /// Parameter `i` (ab 0) des Teils `name`; der Fehler nennt Teil und Stelle (ab 1), die
    /// Zeile setzt [`crate::read::index::StepFile::fail`] davor.
    pub fn arg(&self, name: &str, i: usize) -> Result<&Value, String> {
        let args = self
            .part(name)
            .ok_or_else(|| format!("Der Eintrag ist kein {name}."))?;
        args.get(i)
            .ok_or_else(|| format!("{name}: Der {}. Parameter fehlt.", i + 1))
    }

    /// Typnamen in der Reihenfolge der Datei.
    pub fn names(&self) -> Vec<&str> {
        self.records().iter().map(|r| r.name.as_str()).collect()
    }
}
