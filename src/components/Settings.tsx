import { useState } from 'react';
import type { Controller } from '../useHomePanel';
import type { Preferences } from '../types';
import { Icon } from './Icon';
import { message } from '../bridge';
const regions = [['cn', '中国大陆'], ['tw', '台湾'], ['sg', '新加坡'], ['de', '欧洲'], ['us', '美国'], ['ru', '俄罗斯'], ['i2', '印度']];

export function Settings({ controller: c }: { controller: Controller }) {
  const [saving, setSaving] = useState(false);
  const [confirmLogout, setConfirmLogout] = useState(false);
  const [imageModel, setImageModel] = useState('');
  const [imageUrl, setImageUrl] = useState('');
  const [permissionUrl, setPermissionUrl] = useState('');
  const p = c.data.preferences;
  async function save(next: Preferences) {
    setSaving(true); try { await c.save(next); } catch (e) { c.notify(e); } finally { setSaving(false); }
  }
  return <section className="settings-view">
    <header className="view-header"><button className="icon-button" aria-label="返回设备" onClick={() => c.setSettings(false)}><Icon name="back" /></button><h2>设置</h2><span /></header>
    <div className="settings-scroll"><fieldset disabled={saving}>
      <h3>外观</h3>
      <label className="settings-row"><span>主题</span><select aria-label="主题" value={p.theme} onChange={e => { void save({ ...p, theme: e.target.value }); }}><option value="system">跟随系统</option><option value="light">浅色</option><option value="dark">深色</option></select></label>
      <label className="settings-row"><span>毛玻璃效果<small>{c.data.appearance.glassAvailable ? '使用系统背景模糊' : '当前系统将使用不透明面板'}</small></span><input type="checkbox" role="switch" aria-label="毛玻璃效果" checked={p.glass} onChange={e => { void save({ ...p, glass: e.target.checked }); }} /></label>
      <h3>连接与启动</h3>
      <label className="settings-row"><span>米家服务地区<small>应与米家 App 的地区一致</small></span><select aria-label="米家服务地区" value={p.region} onChange={e => { void save({ ...p, region: e.target.value }); }}>{regions.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select></label>
      <label className="settings-row"><span>开机启动<small>登录 Windows 后驻留托盘</small></span><input type="checkbox" role="switch" aria-label="开机启动" checked={p.autostart} onChange={e => { void save({ ...p, autostart: e.target.checked }); }} /></label>
      <h3>设备图片</h3><p className="settings-description">优先显示已登记且确认获准使用的官方图片。未登记、授权不明或加载失败时显示分类图标，设备控制不受影响。</p>
      <details className="image-policy"><summary>管理有使用许可的图片来源</summary><p className="settings-description">请先确认你有权在此客户端使用该图片。登记地址用于记录来源，本应用不会自动判断图片授权，也不会把图片打包进开源仓库。</p>
        {p.approvedImages.map(image => <div className="image-source-row" key={image.model}><span>{image.model}</span><button type="button" className="text-button" aria-label={`移除 ${image.model} 图片来源`} onClick={() => { void save({ ...p, approvedImages: p.approvedImages.filter(i => i.model !== image.model) }); }}>移除</button></div>)}
        <label className="stack-label">设备型号<input value={imageModel} placeholder="例如 yeelink.light.example" onChange={e => setImageModel(e.target.value)} /></label>
        <label className="stack-label">官方 HTTPS 图片地址<input type="url" value={imageUrl} placeholder="https://…" onChange={e => setImageUrl(e.target.value)} /></label>
        <label className="stack-label">允许使用的授权依据地址<input type="url" value={permissionUrl} placeholder="https://…" onChange={e => setPermissionUrl(e.target.value)} /></label>
        <button type="button" className="secondary-button" disabled={!imageModel || !imageUrl || !permissionUrl} onClick={() => { void save({ ...p, approvedImages: [...p.approvedImages.filter(i => i.model !== imageModel.trim()), { model: imageModel.trim(), url: imageUrl.trim(), permissionUrl: permissionUrl.trim() }] }); }}>登记图片来源</button>
      </details>
      <h3>账号</h3><p className="settings-description">会话仅保存在本机，并由 Windows 加密保护。HomePanel 不保存账号密码，也不上传账号信息到开发者服务器。</p>
      {c.data.authenticated ? <button className="secondary-button danger-text" onClick={() => setConfirmLogout(true)}>退出当前账号</button> : <button className="secondary-button" onClick={() => c.setSettings(false)}>返回登录</button>}
      <div className="about"><strong>HomePanel</strong><span>v{c.data.version} · MIT</span><p>独立开源客户端，与小米公司无隶属关系。</p></div>
    </fieldset></div>
    {confirmLogout ? <div className="confirm-overlay"><section role="alertdialog" aria-labelledby="logout-title" className="confirm-dialog"><h3 id="logout-title">退出当前账号？</h3><p>仅清除 HomePanel 保存在本机的账号会话。设备和米家 App 不受影响。</p><div><button className="secondary-button" autoFocus onClick={() => setConfirmLogout(false)}>取消</button><button className="primary-button" onClick={() => { setSaving(true); void c.logout().catch(e => c.setError(message(e))).finally(() => { setSaving(false); setConfirmLogout(false); }); }}>退出账号</button></div></section></div> : null}
  </section>;
}
