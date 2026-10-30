//! Werte und Einträge einer STEP-Datei, wie `StepFile::entity` sie liefert. Typnamen stehen in
//! Großbuchstaben. Die Zugriffe melden Fehler als `String` (Grund ohne Zeile); der Aufrufer
//! setzt die Zeile davor.
use planora_i18n::tr;

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
    fn kind(&self) -> String {
        match self {
            Value::Int(_) => tr!("step-kind-int"),
            Value::Real(_) => tr!("step-kind-real"),
            Value::Str(_) => tr!("step-kind-string"),
            Value::Enum(_) => tr!("step-kind-enum"),
            Value::Ref(_) => tr!("step-kind-ref"),
            Value::List(_) => tr!("step-kind-list"),
            Value::Typed(..) => tr!("step-kind-typed"),
            Value::Unset => tr!("step-kind-unset"),
            Value::Derived => tr!("step-kind-derived"),
        }
    }

    fn wrong(&self, want: String) -> String {
        tr!("step-expected-found", found = self.kind(), want = want)
    }

    /// Zahl; nimmt auch ganze Zahlen (nachsichtig).
    pub fn real(&self) -> Result<f64, String> {
        match self {
            Value::Real(x) => Ok(*x),
            Value::Int(i) => Ok(*i as f64),
            other => Err(other.wrong(tr!("step-kind-real"))),
        }
    }

    /// Zahl, auch als getypter Wert mit genau einer Zahl (`LENGTH_MEASURE(1.E-3)`,
    /// `PLANE_ANGLE_MEASURE(0.01745)`); den Typnamen prüft der Aufrufer bei Bedarf selbst.
    pub fn measure(&self) -> Result<f64, String> {
        match self {
            Value::Typed(_, args) if args.len() == 1 => args[0].real(),
            Value::Typed(name, _) => Err(tr!("step-expected-one-number", name = name)),
            other => other.real(),
        }
    }

    pub fn int(&self) -> Result<i64, String> {
        match self {
            Value::Int(i) => Ok(*i),
            other => Err(other.wrong(tr!("step-kind-int"))),
        }
    }

    pub fn reference(&self) -> Result<u64, String> {
        match self {
            Value::Ref(r) => Ok(*r),
            other => Err(other.wrong(tr!("step-kind-ref"))),
        }
    }

    pub fn list(&self) -> Result<&[Value], String> {
        match self {
            Value::List(items) => Ok(items),
            other => Err(other.wrong(tr!("step-kind-list"))),
        }
    }

    pub fn text(&self) -> Result<&str, String> {
        match self {
            Value::Str(s) => Ok(s),
            other => Err(other.wrong(tr!("step-kind-string"))),
        }
    }

    pub fn enumeration(&self) -> Result<&str, String> {
        match self {
            Value::Enum(e) => Ok(e),
            other => Err(other.wrong(tr!("step-kind-enum"))),
        }
    }

    /// `.T.` → `Some(true)`, `.F.` → `Some(false)`, `.U.` → `None`.
    pub fn logical(&self) -> Result<Option<bool>, String> {
        match self.enumeration()? {
            "T" | "TRUE" => Ok(Some(true)),
            "F" | "FALSE" => Ok(Some(false)),
            "U" | "UNKNOWN" => Ok(None),
            other => Err(tr!("step-expected-logical", other = other)),
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
            .ok_or_else(|| tr!("step-entry-wrong-kind", name = name))?;
        args.get(i)
            .ok_or_else(|| tr!("step-entry-missing-arg", n = i + 1, name = name))
    }

    /// Typnamen in der Reihenfolge der Datei.
    pub fn names(&self) -> Vec<&str> {
        self.records().iter().map(|r| r.name.as_str()).collect()
    }
}
