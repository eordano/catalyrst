import { Dialog } from './Dialog';
import { Avatar, useProfile } from './Profile';
import { shortWallet } from './api';
export function ProfileDialog({ wallet, self, onClose }: { wallet: string; self?: string; onClose: () => void }) {
  const profile = useProfile(wallet);
  return <Dialog title="Profile" onClose={onClose} className="profile-dialog"><div className="profile-cover" /><div className="profile-content"><Avatar wallet={wallet} /><h2>{profile?.name || shortWallet(wallet)}</h2><small className="wallet-address">{wallet}</small>{profile?.description && <p>{profile.description}</p>}<div className="profile-actions"><a className="outline-button" href={`https://decentraland.org/profile/accounts/${wallet}`} target="_blank" rel="noreferrer">View Decentraland profile &#x2197;</a>{wallet.toLowerCase() !== self?.toLowerCase() && <a className="primary" href={`#/dm/${wallet}`} onClick={onClose}>Message</a>}{self && wallet.toLowerCase() !== self.toLowerCase() && <button className="outline-button" onClick={() => {onClose();window.dispatchEvent(new CustomEvent("social:call",{detail:wallet.toLowerCase()}));}}>Call</button>}</div></div></Dialog>;
}
