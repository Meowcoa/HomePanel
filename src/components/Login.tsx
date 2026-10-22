import { useEffect, useState } from 'react';
import type { Controller } from '../useHomePanel';
import { demo } from '../bridge';
import { Icon } from './Icon';

export function Login({ controller: c }: { controller: Controller }) {
  const [requesting, setRequesting] = useState(false);
  const [now, setNow] = useState(Date.now());
  const login = c.data.login;
  useEffect(() => { if (login.stage !== 'waiting') return; const timer = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(timer); }, [login.stage]);
  const seconds = Math.max(0, Math.ceil(((login.expiresAt ?? now) - now) / 1000));
  const waiting = login.stage === 'waiting' && seconds > 0;
  return <section className="login-view">
    <header className="panel-header"><div className="brand"><span className="brand-icon"><Icon name="home" size={23} /></span><h1>HomePanel</h1></div><button className="icon-button" aria-label="设置" onClick={() => c.setSettings(true)}><Icon name="settings" /></button></header>
    <div className="login-content"><div className="login-stage"><div className="login-stage-meta"><span>HOME SYSTEM</span><span className="login-ready"><i /> PRIVATE BY DESIGN</span></div><div className="login-visual" aria-hidden="true"><span className="login-orbit orbit-a" /><span className="login-orbit orbit-b" /><span className="login-visual-core"><Icon name="home" size={39} /></span><span className="login-visual-node"><Icon name="power" size={14} /></span><span className="login-visual-spark spark-a" /><span className="login-visual-spark spark-b" /></div><div className="login-copy"><span className="login-overline">YOUR HOME, IN REACH</span><h2>家的状态，<br />尽在掌握</h2><p>连接米家账号后，常用设备会在桌面托盘旁等你。<br />扫码授权，随时查看与控制。</p></div></div>
      {waiting && login.qr ? <div className="qr-area"><div className="qr-image"><img src={login.qr} alt="使用米家 App 扫描此二维码" /></div><strong>请使用米家 App 扫码</strong><span>请在手机上确认登录 · {seconds} 秒后过期</span></div> : <div className="connect-area"><button className="primary-button connect-button" disabled={requesting} onClick={() => { setRequesting(true); void c.startLogin().finally(() => setRequesting(false)); }}>{requesting ? <span className="spinner" /> : <Icon name={login.stage ? 'refresh' : 'qr'} size={21} />}{requesting ? '正在获取二维码' : login.stage ? '刷新登录二维码' : '连接米家账号'}</button><p className="muted-note">{login.message || '首次使用请完成一次扫码登录'}</p></div>}
      {demo ? <span className="demo-caption">演示模式无法连接真实账号</span> : null}
    </div><footer className="login-footer">独立开源客户端 · 账号会话仅保存在本机</footer>
  </section>;
}
