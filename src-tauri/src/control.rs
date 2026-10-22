//! Independently implemented MIoT property acknowledgement and fresh confirmation.
use crate::model::string_id;
use serde_json::{json, Value};
use std::{future::Future, time::Duration};

pub struct PropertyTarget<'a> {
    pub did: &'a str,
    pub siid: u32,
    pub piid: u32,
    pub time_period: bool,
}

impl PropertyTarget<'_> {
    fn values_match(&self, actual:&Value, expected:&Value)->bool {
        if !self.time_period { return actual==expected; }
        let parse=|value:&Value|value.as_str().and_then(crate::time_range::TimeRange::parse);
        match (parse(actual),parse(expected)) { (Some(a),Some(b))=>a==b, _=>false }
    }
    fn row<'a>(&self, result: &'a Value) -> Option<&'a Value> {
        let mut matches = result.as_array()?.iter().filter(|row|
            string_id(&row["did"]) == self.did &&
            row["siid"].as_u64() == Some(self.siid as u64) &&
            row["piid"].as_u64() == Some(self.piid as u64));
        let row = matches.next()?;
        matches.next().is_none().then_some(row)
    }

    pub fn fresh_request(&self) -> Value {
        json!({"params":[{"did":self.did,"siid":self.siid,"piid":self.piid}],"datasource":2})
    }

    fn acknowledge(&self, result: &Value) -> Result<(), String> {
        match self.row(result).and_then(|row| row["code"].as_i64()) {
            // Nested property-set codes only. Neither means execution is confirmed.
            Some(0 | 1) => Ok(()),
            Some(code) => Err(format!("设备未接受操作（{code}），请稍后重试")),
            None => Err("UNCERTAIN: 操作响应不完整，状态未确认，请刷新查看".into()),
        }
    }

    pub fn read_value(&self, result: &Value) -> Option<Value> {
        let row = self.row(result)?;
        (row["code"].as_i64() == Some(0) && !row["value"].is_null())
            .then(|| row["value"].clone())
    }
}

/// The write future is consumed exactly once. Only reads may be retried.
pub async fn write_and_confirm<W, R, F>(
    target: &PropertyTarget<'_>, expected: &Value, readable: bool,
    write: W, mut read: R, budget: Duration, delays: &[u64],
) -> Result<Value, String>
where
    W: Future<Output = Result<Value, String>>,
    R: FnMut() -> F,
    F: Future<Output = Result<Value, String>>,
{
    let accepted = match write.await {
        Ok(result) => { target.acknowledge(&result)?; true }
        Err(error) if error.starts_with("AUTH_REQUIRED") || error.starts_with("账号状态已改变") || error.starts_with("设备服务拒绝了请求") => return Err(error),
        // A transport failure may occur after the device has applied the write.
        Err(_) => false,
    };
    if !readable {
        return Err(if accepted {
            "UNCERTAIN: 命令已受理，此属性不支持回读，无法确认状态"
        } else { "UNCERTAIN: 发送结果未确认，此属性不支持回读，请在米家中查看" }.into());
    }
    let confirmation = async {
        for delay in delays {
            tokio::time::sleep(Duration::from_millis(*delay)).await;
            match read().await {
                Ok(result) => {
                    if let Some(value) = target.read_value(&result) {
                        if target.values_match(&value, expected) { return Ok(value); }
                    }
                }
                Err(error) if error.starts_with("AUTH_REQUIRED") || error.starts_with("账号状态已改变") => return Err(error),
                Err(_) => {}
            }
        }
        Err("UNCERTAIN: 操作结果尚未确认，请刷新查看；程序不会自动重复发送".into())
    };
    tokio::time::timeout(budget, confirmation).await
        .unwrap_or_else(|_| Err("UNCERTAIN: 状态确认超时，请刷新查看；程序不会自动重复发送".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn only_schedule_properties_accept_equivalent_minute_and_second_echoes() {
        let mut scheduled=target();scheduled.time_period=true;
        assert!(scheduled.values_match(&json!("22:00-06:00"),&json!("22:00:00-06:00:00")));
        assert!(!scheduled.values_match(&json!("22:00:01-06:00:00"),&json!("22:00:00-06:00:00")));
        assert!(!target().values_match(&json!("22:00-06:00"),&json!("22:00:00-06:00:00")));
        assert!(!scheduled.values_match(&json!("invalid"),&json!("invalid")));
    }

    fn target() -> PropertyTarget<'static> { PropertyTarget { did: "test-device", siid: 2, piid: 1, time_period:false } }
    fn row(code: i64, value: Value) -> Value {
        json!([{"did":"test-device","siid":2,"piid":1,"code":code,"value":value}])
    }
    fn run<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
    }

    #[test]
    fn acknowledgement_requires_one_matching_target_and_explicit_code() {
        for code in [0, 1] { assert!(target().acknowledge(&row(code, Value::Null)).is_ok()); }
        for result in [json!([]), json!({"code":0}), json!([{"code":0}]),
            json!([{"did":"other","siid":2,"piid":1,"code":0}]),
            json!([{"did":"test-device","siid":2,"piid":9,"code":0}]),
            json!([{"did":"test-device","siid":2,"piid":1}]),
            json!([row(0, Value::Null)[0], row(1, Value::Null)[0]])] {
            assert!(target().acknowledge(&result).unwrap_err().starts_with("UNCERTAIN"));
        }
        assert!(target().acknowledge(&row(-704030013, Value::Null)).unwrap_err().contains("-704030013"));
        let mixed = json!([{"did":"other","code":-1}, row(1, Value::Null)[0]]);
        assert!(target().acknowledge(&mixed).is_ok());
    }

    #[test]
    fn both_accepted_codes_require_fresh_confirmation() {
        for code in [0, 1] {
            let reads = Cell::new(0);
            let result = run(write_and_confirm(&target(), &json!(true), true, async { Ok(row(code, Value::Null)) },
                || { reads.set(reads.get()+1); async { Ok(row(0, json!(true))) } }, Duration::from_secs(1), &[0]));
            assert_eq!(result.unwrap(), json!(true));
            assert_eq!(reads.get(), 1);
        }
        let request = target().fresh_request();
        assert_eq!(request["datasource"], 2);
        assert_eq!(request["params"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn stale_wrong_missing_and_failed_reads_never_confirm() {
        for result in [row(0, json!(false)), row(1, json!(true)), row(0, Value::Null),
            json!([{"did":"other","siid":2,"piid":1,"code":0,"value":true}]),
            json!([{"did":"test-device","siid":2,"piid":9,"code":0,"value":true}]), json!([])] {
            let outcome = run(write_and_confirm(&target(), &json!(true), true, async { Ok(row(1, Value::Null)) },
                || async { Ok(result.clone()) }, Duration::from_secs(1), &[0]));
            assert!(outcome.unwrap_err().starts_with("UNCERTAIN"));
        }
    }

    #[test]
    fn malformed_or_rejected_acknowledgements_do_not_enter_read_confirmation() {
        for result in [json!([]), json!([{"code":1}]), row(-1, Value::Null)] {
            assert!(run(write_and_confirm(&target(), &json!(true), true, async { Ok(result) },
                || async { panic!("unaccepted acknowledgement") }, Duration::from_secs(1), &[0])).is_err());
        }
    }

    #[test]
    fn write_is_sent_once_while_reads_retry_and_network_failure_can_be_confirmed() {
        for transport_failure in [false, true] {
            let writes = Cell::new(0);
            let reads = Cell::new(0);
            let result = run(write_and_confirm(&target(), &json!(true), true, async {
                writes.set(writes.get()+1);
                if transport_failure { Err("连接超时".into()) } else { Ok(row(1, Value::Null)) }
            }, || { reads.set(reads.get()+1); let n=reads.get(); async move {
                if n==1 { Err("读取失败".into()) } else { Ok(row(0, json!(true))) }
            } }, Duration::from_secs(1), &[0,0]));
            assert!(result.is_ok());
            assert_eq!(writes.get(), 1);
            assert_eq!(reads.get(), 2);
        }
    }

    #[test]
    fn auth_expiry_denial_and_unreadable_properties_stop_confirmation() {
        for error in ["AUTH_REQUIRED: 登录失效", "账号状态已改变，请重新刷新", "设备服务拒绝了请求（1）"] {
            let result = run(write_and_confirm(&target(), &json!(true), true, async { Err(error.into()) },
                || async { panic!("unexpected read") }, Duration::from_secs(1), &[0]));
            assert_eq!(result.unwrap_err(), error);
        }
        let result = run(write_and_confirm(&target(), &json!(true), true, async { Ok(row(0, Value::Null)) },
            || async { Err("AUTH_REQUIRED: 登录失效".into()) }, Duration::from_secs(1), &[0,0]));
        assert!(result.unwrap_err().starts_with("AUTH_REQUIRED"));
        let result = run(write_and_confirm(&target(), &json!(true), false, async { Ok(row(1, Value::Null)) },
            || async { panic!("unreadable property") }, Duration::from_secs(1), &[0]));
        assert!(result.unwrap_err().starts_with("UNCERTAIN"));
    }

    #[test]
    fn bounded_confirmation_handles_hung_reads_and_preserves_uncertainty() {
        let started = std::time::Instant::now();
        let result = run(write_and_confirm(&target(), &json!(true), true, async { Ok(row(1, Value::Null)) },
            || async { std::future::pending().await }, Duration::from_millis(20), &[0]));
        assert!(result.unwrap_err().contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
