//! Probe sonic_rs::Value iteration API.
use sonic_rs::{JsonContainerTrait, JsonValueTrait, Value};

fn main() {
    let json = br#"{"a": 1, "b": [1, 2], "c": {"d": "hi"}}"#;
    let v: Value = sonic_rs::from_slice(json).unwrap();
    let obj = v.as_object().unwrap();
    for (k, val) in obj.iter() {
        println!(
            "key={} is_arr={} is_obj={} is_str={} is_num={} is_bool={} is_null={}",
            k,
            val.is_array(),
            val.is_object(),
            val.is_str(),
            val.is_number(),
            val.is_boolean(),
            val.is_null()
        );
    }
    let arr = obj.get(&"b").unwrap().as_array().unwrap();
    println!("array len: {}", arr.len());
    for (i, item) in arr.iter().enumerate() {
        println!("  [{}] as_i64={:?}", i, item.as_i64());
    }
    // Check Number types
    let n = obj.get(&"a").unwrap();
    println!("a: is_i64={} as_i64={:?} as_f64={:?}", n.is_i64(), n.as_i64(), n.as_f64());
    // str access
    let c = obj.get(&"c").unwrap();
    let c_obj = c.as_object().unwrap();
    let d = c_obj.get(&"d").unwrap();
    println!("d: is_str={} as_str={:?}", d.is_str(), d.as_str());
}
