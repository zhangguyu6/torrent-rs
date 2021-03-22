mod de;
mod ser;
mod value;

pub use de::{from_bytes, from_str, Deserializer};
pub use ser::{to_bytes, to_str, Serializer};
pub use value::Value;

#[cfg(test)]
mod tests {

    use super::{
        de::{from_bytes, from_str},
        ser::{to_bytes, to_str},
        value::Value,
    };

    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    fn test_value_ser_de<T: Into<Value>>(v: T) {
        let a: Value = v.into();
        println!(
            "value is {:?} , bytes is {:?}",
            a,
            String::from_utf8_lossy(&to_bytes(&a).unwrap())
        );
        let b: Value = from_bytes(&to_bytes(&a).unwrap()).unwrap();
        assert_eq!(a, b);
    }
    fn test_value_de_ser(s: &str) {
        let d: Value = from_bytes(s.as_bytes()).unwrap();
        let e = to_bytes(&d).unwrap();
        assert_eq!(Vec::from(s.as_bytes()), e);
    }
    fn test_ser_de<'a, T: Serialize + Deserialize<'a> + std::fmt::Debug + Eq>(a: &T) {
        let buf = to_bytes(a).unwrap();
        println!("{:?}", to_str(a));
        let b = from_bytes::<T>(buf.as_ref()).unwrap();
        println!("{:?}", to_str(&b));
        assert_eq!(a, &b);
    }
    #[test]
    fn test_ser_de_int() {
        test_value_ser_de(1);
        test_value_ser_de(128845848);
        test_value_de_ser("i0e");
        test_value_de_ser("i1e");
        test_value_de_ser("i-128e");
    }
    #[test]
    fn test_ser_de_string() {
        test_value_ser_de("bencode");
        test_value_ser_de("1234567890");
        test_value_de_ser("7:bencode");
        test_value_de_ser("11:12345678901");
    }
    #[test]
    fn test_ser_de_list() {
        test_value_ser_de(Value::List(vec![1.into(), 2.into(), 3.into()]));
        test_value_ser_de(Value::List(vec![1.into(), "2".into()]));
        let l = Value::List(vec![1.into(), "2".into()]);
        test_value_ser_de(Value::List(vec![1.into(), "2".into(), l]));
        test_value_de_ser("li1ei2ei3ee");
        test_value_de_ser("li1e1:2e");
        test_value_de_ser("li1e1:2li1e1:2ee");
    }
    #[test]
    fn test_ser_de_map() {
        let mut m = BTreeMap::new();
        m.insert("b".into(), 1.into());
        test_value_ser_de(m.clone());
        m.insert("a".into(), "b".into());
        test_value_ser_de(m.clone());
        let l = Value::List(vec![1.into(), "2".into()]);
        m.insert("c".into(), l);
        test_value_ser_de(m.clone());
        let mut new_m = BTreeMap::new();
        new_m.insert("new_m".into(), 1.into());
        m.insert("d".into(), Value::Dict(new_m));
        test_value_ser_de(m.clone());
        test_value_de_ser("d1:bi1ee");
        test_value_de_ser("d1:a1:b1:bi1ee");
        test_value_de_ser("d1:a1:b1:bi1e1:cli1e1:2ee");
        test_value_de_ser("d1:a1:b1:bi1e1:cli1e1:2e1:dd5:new_mi1eee");
    }
    #[test]
    fn test_ser_de_struct_sort() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct File {
            count: i64,
            name: String,
        }
        let f1 = File {
            count: 0,
            name: "hello".into(),
        };
        assert_eq!(to_str(&f1).unwrap(), "d5:counti0e4:name5:helloe");
        assert_eq!(
            from_str::<File>("d5:counti0e4:name5:helloe").unwrap(),
            File {
                count: 0,
                name: "hello".into()
            }
        );
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct A {
            c: i64,
            b: i64,
            a: i64,
        }
        assert_eq!(
            to_str(&A { c: 0, b: 1, a: 2 }).unwrap(),
            "d1:ai2e1:bi1e1:ci0ee"
        );
        assert_eq!(
            from_str::<A>("d1:ai2e1:bi1e1:ci0ee").unwrap(),
            A { c: 0, b: 1, a: 2 }
        );
    }
    #[test]
    fn test_ser_de_struct_opt() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct File {
            count: i64,
            name: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            a: Option<i64>,
        }
        let mut f1 = File {
            count: 0,
            name: "hello".into(),
            a: Some(1),
        };
        assert_eq!(to_str(&f1).unwrap(), "d1:ai1e5:counti0e4:name5:helloe");
        f1.a = None;
        assert_eq!(to_str(&f1).unwrap(), "d5:counti0e4:name5:helloe");
        assert_eq!(
            from_str::<File>("d5:counti0e4:name5:helloe").unwrap(),
            File {
                count: 0,
                name: "hello".into(),
                a: None
            }
        );
        assert_eq!(
            from_str::<File>("d5:counti0e4:name5:hello1:ai1ee").unwrap(),
            File {
                count: 0,
                name: "hello".into(),
                a: Some(1)
            }
        );
    }
    #[test]
    fn test_ser_de_newtype_struct() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct A(i32);
        let a = A(1);
        test_ser_de(&a);
    }
    #[test]
    fn test_ser_de_tuple_struct() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct A(i32, i64);
        let a = A(1, 2);
        test_ser_de(&a);
    }
    #[test]
    fn test_ser_de_tuple() {
        // https://github.com/serde-rs/serde/issues/1413
        // Deserialize &str is difficult
        let a = (1, "a".to_string());
        test_ser_de(&a);
    }
    #[test]
    fn test_ser_de_variant_unit() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        enum V {
            A,
            B,
        }
        test_ser_de(&V::A);
    }
    #[test]
    fn test_ser_de_variant_newtype() {
        #[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
        enum V {
            A(i64),
            B(i64),
        };
        test_ser_de(&V::A(0));
    }
    #[test]
    fn test_ser_de_variant_tuple() {
        #[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
        enum V {
            A(i64, i64),
            B(i64, i64),
        };
        test_ser_de(&V::A(0, 1));
    }
    #[test]
    fn test_ser_de_variant_struct() {
        #[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
        enum V {
            A { a: i64, b: i64 },
            B { c: i64, d: i64 },
        };
        test_ser_de(&V::A { a: 0, b: 1 });
    }
}
