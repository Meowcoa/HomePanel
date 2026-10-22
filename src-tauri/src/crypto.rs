//! Cloud wire format adapted from the MIT-licensed mihome-ctl connector.
//! See THIRD_PARTY_NOTICES.md. RC4 is required by the remote wire protocol;
//! local session storage uses Windows DPAPI, never RC4.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use sha1::Sha1;
use sha2::{Digest, Sha256};

pub fn nonce_at(milliseconds: u64, random: [u8; 8]) -> String {
    let mut bytes = Vec::from(random);
    bytes.extend_from_slice(&((milliseconds / 60_000) as u32).to_be_bytes());
    B64.encode(bytes)
}
pub fn nonce() -> String {
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    nonce_at(crate::now_ms(), bytes)
}
pub fn signed_nonce(secret: &str, nonce: &str) -> Result<String, String> {
    let mut input = B64.decode(secret).map_err(|_| "账号凭据格式无效，请重新扫码")?;
    input.extend(B64.decode(nonce).map_err(|_| "请求签名生成失败")?);
    Ok(B64.encode(Sha256::digest(input)))
}
fn signature(path: &str, fields: &[(String, String)], key: &str) -> String {
    let mut parts = vec!["POST".to_string(), path.to_string()];
    parts.extend(fields.iter().map(|(k,v)| format!("{k}={v}")));
    parts.push(key.to_string());
    B64.encode(Sha1::digest(parts.join("&").as_bytes()))
}
pub fn rc4_drop1024(key: &[u8], bytes: &[u8]) -> Result<Vec<u8>, String> {
    if key.is_empty() { return Err("加密参数无效".into()); }
    let mut state = [0u8; 256];
    for (i,v) in state.iter_mut().enumerate() { *v = i as u8; }
    let mut j = 0usize;
    for i in 0..256 { j = (j + state[i] as usize + key[i % key.len()] as usize) & 255; state.swap(i,j); }
    let (mut i,mut j) = (0usize,0usize);
    let mut output = Vec::with_capacity(bytes.len());
    for n in 0..(1024 + bytes.len()) {
        i = (i+1)&255; j = (j+state[i] as usize)&255; state.swap(i,j);
        let mask = state[(state[i] as usize+state[j] as usize)&255];
        if n >= 1024 { output.push(bytes[n-1024] ^ mask); }
    }
    Ok(output)
}
pub fn fields_at(path: &str, data: &str, secret: &str, nonce: &str) -> Result<Vec<(String,String)>,String> {
    let key = signed_nonce(secret,nonce)?;
    let mut fields = vec![("data".into(),data.into())];
    let hash = signature(path,&fields,&key);
    fields.push(("rc4_hash__".into(),hash));
    let bytes = B64.decode(&key).map_err(|_| "请求签名生成失败")?;
    for (_,value) in &mut fields { *value = B64.encode(rc4_drop1024(&bytes,value.as_bytes())?); }
    let sig = signature(path,&fields,&key);
    fields.extend([("signature".into(),sig),("ssecurity".into(),secret.into()),("_nonce".into(),nonce.into())]);
    Ok(fields)
}
pub fn decrypt(secret: &str, nonce: &str, body: &str) -> Result<Vec<u8>,String> {
    let key = B64.decode(signed_nonce(secret,nonce)?).map_err(|_| "云端响应解码失败")?;
    let bytes = B64.decode(body.trim()).map_err(|_| "云端响应格式已变化，请稍后重试")?;
    rc4_drop1024(&key,&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn nonce_has_minutes_in_big_endian() { assert_eq!(B64.decode(nonce_at(120_000,[1;8])).unwrap(),[1,1,1,1,1,1,1,1,0,0,0,2]); }
    #[test] fn encrypted_payload_roundtrips() {
        let secret = B64.encode(b"0123456789abcdef"); let n=nonce_at(120_000,[1;8]);
        let fields=fields_at("/miotspec/prop/get","{\"params\":[]}",&secret,&n).unwrap();
        assert_eq!(decrypt(&secret,&n,&fields[0].1).unwrap(),b"{\"params\":[]}");
        assert_ne!(fields[0].1,"{\"params\":[]}");
        assert_eq!(fields.iter().map(|x|x.0.as_str()).collect::<Vec<_>>(),["data","rc4_hash__","signature","ssecurity","_nonce"]);
    }
    #[test] fn empty_key_rejected() { assert!(rc4_drop1024(&[],b"x").is_err()); }
}
