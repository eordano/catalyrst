const assert = require('node:assert/strict');
const {mkdtempSync,rmSync}=require('node:fs');
const path=require('node:path');
const http=require('node:http');
const {WebSocketServer}=require('ws');
const {createRpcServer}=require('@dcl/rpc/dist/server');
const {WebSocketTransport}=require('@dcl/rpc/dist/transports/WebSocket');
const {registerService}=require('@dcl/rpc/dist/codegen');
const {SocialServiceDefinition}=require('@dcl/protocol/out-js/decentraland/social_service/v2/social_service_v2.gen');
const {Packet}=require('@dcl/protocol/out-js/decentraland/kernel/comms/rfc4/comms.gen');
const {chromium}=require('playwright');
const peer='0x0000000000000000000000000000000000000002';
const own='0x0000000000000000000000000000000000000001';
const other='0x0000000000000000000000000000000000000003';
const community='11111111-1111-4111-8111-111111111111';
const friend={address:peer,name:'Ada',hasClaimedName:true,profilePictureUrl:''};
const streams=[]; const calls=[];
function stream() {
 const values=[]; let wake;
 const push=value=>{values.push(value);wake?.();};
 return {push,async *[Symbol.asyncIterator](){while(true){if(!values.length)await new Promise(r=>wake=r);while(values.length)yield values.shift();}}};
}
(async()=>{
 const {build,preview}=await import('vite');
 const dir=mkdtempSync('/tmp/dcl-friends-test-');
 let server,browser,web,wss;
 try {
 await build({configFile:false,root:path.resolve(__dirname,'..'),base:'/',logLevel:'error',resolve:{alias:{'livekit-client':path.join(__dirname,'fixtures/livekit.ts')}},build:{outDir:dir,emptyOutDir:true}});
 server=await preview({configFile:false,root:path.resolve(__dirname,'..'),build:{outDir:dir},preview:{host:'127.0.0.1',port:0}});
 const origin=`http://127.0.0.1:${server.httpServer.address().port}`;
 web=http.createServer(); wss=new WebSocketServer({server:web}); await new Promise(r=>web.listen(0,'127.0.0.1',r));
 let blocked=false,heartbeats=0,connections=0,sentRequests=[];
 let privacyFailure=false;
 let privacy={privateMessagesPrivacy:1,blockedUsersMessagesVisibility:1,showSituationReactions:0};
 wss.on('connection',socket=>{connections++;socket.once('message',raw=>{
  const auth=JSON.parse(raw.toString()); assert.ok(auth['x-identity-auth-chain-0']);
  const rpc=createRpcServer({});
  rpc.setHandler(async port=>registerService(port,SocialServiceDefinition,async()=>new Proxy({
   getSocialSettings:async()=>privacyFailure?{}:({response:{$case:'ok',ok:{settings:privacy}}}),
   upsertSocialSettings:async settings=>{privacy=settings;calls.push(['privacy',settings]);return {response:{$case:'ok',ok:privacy}};},
   promoteSpeakerInCommunityVoiceChat:async target=>{calls.push(['promote',target]);return {response:{$case:'ok',ok:{message:'ok'}}}},
   demoteSpeakerInCommunityVoiceChat:async target=>{calls.push(['demote',target]);return {response:{$case:'ok',ok:{message:'ok'}}}},
   rejectSpeakRequestInCommunityVoiceChat:async target=>{calls.push(['reject-hand',target]);return {response:{$case:'ok',ok:{message:'ok'}}}},
   kickPlayerFromCommunityVoiceChat:async target=>{calls.push(['kick',target]);return {response:{$case:'ok',ok:{message:'ok'}}}},
   muteSpeakerFromCommunityVoiceChat:async target=>{calls.push(['host-mute',target]);return {response:{$case:'ok',ok:{muted:target.muted}}}},
   endCommunityVoiceChat:async target=>{calls.push(['end-community',target]);return {response:{$case:'ok',ok:{message:'ok'}}}},
   subscribeToPrivateVoiceChatUpdates:async function*(){const s=stream();s.voice=true;streams.push(s);yield* s;},
   getIncomingPrivateVoiceChatRequest:async()=>({response:{$case:'notFound',notFound:{}}}),
   startPrivateVoiceChat:async({callee})=>{calls.push(['start',callee.address]);return {response:{$case:'ok',ok:{callId:'outgoing'}}};},
   acceptPrivateVoiceChat:async({callId})=>{calls.push(['accept',callId]);return {response:{$case:'ok',ok:{callId,credentials:{connectionUrl:'livekit:wss://voice.test.invalid?access_token=fixture'}}}};},
   rejectPrivateVoiceChat:async({callId})=>{calls.push(['reject',callId]);return {response:{$case:'ok',ok:{callId}}};},
   endPrivateVoiceChat:async({callId})=>{calls.push(['end',callId]);return {response:{$case:'ok',ok:{callId}}};},
   getSentFriendshipRequests:async()=>({response:{$case:'requests',requests:{requests:sentRequests}}}),
   getMutualFriends:async()=>({friends:[friend],paginationData:{total:1,page:1}}),
   blockUser:async()=>{blocked=true;return {response:{$case:'ok',ok:{profile:friend}}}},
   unblockUser:async()=>{blocked=false;return {response:{$case:'ok',ok:{}}}},
   upsertFriendship:async({action})=>{calls.push(['friendship',action.$case]);if(action.$case==='request')sentRequests=[{id:'sent',friend:{...friend,address:action.request.user.address,name:'New friend'},createdAt:Date.now()}];if(action.$case==='cancel')sentRequests=[];return {response:{$case:'accepted',accepted:{id:'updated',createdAt:Date.now(),friend}}}},
   getPendingFriendshipRequests:async()=>({response:{$case:'requests',requests:{requests:[]}}}),
   joinCommunityVoiceChat:async()=>({response:{$case:'ok',ok:{voiceChatId:'fixture',credentials:{connectionUrl:'livekit:wss://voice.test.invalid?access_token=fixture'}}}}),
   startCommunityVoiceChat:async()=>({response:{$case:'ok',ok:{credentials:{connectionUrl:'livekit:wss://voice.test.invalid?access_token=fixture'}}}}),
   requestToSpeakInCommunityVoiceChat:async()=>({response:{$case:'ok',ok:{}}}),
   getFriends:async()=>{return {friends:[{...friend,address:'0x0000000000000000000000000000000000000004',name:'Aaron'},friend],paginationData:{total:2,page:1}}},
   getFriendshipStatus:async()=>{heartbeats++;return {response:{$case:'accepted',accepted:{status:7}}};},
   getBlockingStatus:async()=>{return {blockedUsers:blocked?[peer]:[],blockedByUsers:[]}},
   subscribeToCommunityMemberConnectivityUpdates:async function*(){const s=stream();streams.push(s);yield* s;},
   subscribeToFriendConnectivityUpdates:async function*(){yield {friend,status:0}; const s=stream(); streams.push(s);yield* s;},
   subscribeToFriendshipUpdates:async function*(){const s=stream();streams.push(s);yield* s;},
   subscribeToBlockUpdates:async function*(){const s=stream();streams.push(s);s.block=true;yield* s;},
  }, { get: (target, key) => key === "then" ? undefined : target[key] || (async () => ({})) })));
  rpc.attachTransport(WebSocketTransport(socket),{});
 });});
 browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||chromium.executablePath(),args:['--no-sandbox','--use-fake-device-for-media-stream','--use-fake-ui-for-media-stream']});
 const page=await browser.newPage({viewport:{width:1440,height:950}});
 const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.log('page error:',e.message)});page.on('console',m=>{if(m.type()==='error')console.log('browser:',m.text())});
 await page.addInitScript(({own,port})=>{
  window.dclSocialIdentity={address:own,canSignSilently:true,signRequest:async p=>[{type:'SIGNER',payload:own,signature:''},{type:'ECDSA_SIGNED_ENTITY',payload:p.payload,signature:'fixture'}]};
  const Original=window.WebSocket;
  window.WebSocket=class extends Original {constructor(url,...args){super(String(url).includes('rpc-social-service-ea')?`ws://127.0.0.1:${port}`:url,...args)}};
 },{own,port:web.address().port});
 let explorerLocation=null;
 await page.route('**/api/**',route=>{
  const url=new URL(route.request().url());
  if(url.pathname==='/api/locations')return route.fulfill({json:{locations:explorerLocation?{[peer]:explorerLocation}: {}}});
  if(url.pathname==='/api/actions')return route.fulfill({json:{id:route.request().postDataJSON().operation.type,wallet:own,operation:{type:'private_chat_token'},payload:'test',url:'https://comms-gatekeeper.decentraland.org/private-messages/token',method:'GET',metadata:'{}',timestamp:String(Date.now()),expiresAt:Date.now()+90000}});
  if(url.pathname.includes('/my_communities/complete'))return route.fulfill({json:{data:{results:[],total:0}}});
  if(url.pathname.includes('/own_location/complete'))return route.fulfill({json:{location:{x:5,y:6,updatedAt:Date.now()}}});
  if(url.pathname.includes('/community_details/complete'))return route.fulfill({json:{data:{id:community,name:'Genesis Builders',active:true,role:'owner',membersCount:2}}});
  if(url.pathname.includes('/open_community/complete'))return route.fulfill({json:{community:{id:community,name:'Genesis Builders',role:'owner',membersCount:2},channel:'general',messages:[],readToken:'fixture',expiresAt:Date.now()+90000}});
  if(url.pathname.endsWith('/messages'))return route.fulfill({json:{messages:[]}});
  if(url.pathname.endsWith('/complete'))return route.fulfill({json:{adapter:'livekit:wss://test.invalid?access_token=fixture'}});
  if(url.pathname==='/api/communities')return route.fulfill({json:{data:{results:[],total:0}}});
  if(url.pathname==='/api/telemetry')return route.fulfill({status:202,body:''});
  return route.fulfill({json:{data:[]}});
 });
 await page.goto(origin+'/#/friends');
 await page.waitForFunction(()=>window.published?.some(p=>p.options.topic==='dcl.social.location.v1'));
 const location=await page.evaluate(()=>window.published.find(p=>p.options.topic==='dcl.social.location.v1'));
 assert.deepEqual(location.options.destinationIdentities,[peer]);
 assert.equal(JSON.parse(new TextDecoder().decode(Uint8Array.from(location.bytes))).location.x,5);
 assert.ok((await page.locator('.friends-list .friend-row').first().innerText()).includes('Ada'));
 assert.equal(await page.locator('.inbox-row').count(),0);
 await page.getByRole('heading',{name:'Friends in Genesis City'}).waitFor();
 await page.setViewportSize({width:390,height:844});
 await page.getByRole('heading',{name:'Friends in Genesis City'}).waitFor();
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
 await page.setViewportSize({width:1440,height:1000});
 assert.equal(await page.getByRole('checkbox',{name:'Share my location while online'}).count(),0);
 await page.getByRole('link',{name:/Ada Available to chat/}).click().catch(async e=>{console.log(await page.locator('.friends-view').innerText());throw e;});
 const sendLocation=async (location,sender=peer)=>page.evaluate(({location,sender})=>window.room.emit('data',new TextEncoder().encode(JSON.stringify({location})),{identity:sender},undefined,'dcl.social.location.v1'),{location,sender});
 await sendLocation({x:11,y:12,updatedAt:Date.now()});
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor();
 assert.equal(await page.getByRole('link',{name:'Join friend \u2197',exact:true}).getAttribute('href'),'https://decentraland.org/jump/?position=11,12');
 assert.equal(await page.locator('.direct-message').count(),0);
 await sendLocation({x:99,y:99,updatedAt:Date.now()},other);
 assert.equal(await page.getByRole('link',{name:'Join friend \u2197',exact:true}).getAttribute('href'),'https://decentraland.org/jump/?position=11,12');
 await page.evaluate(()=>{const original=Date.now; window.restoreClock=()=>{Date.now=original};Date.now=()=>original()+50000;});
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor({state:'hidden',timeout:10000});
 await page.evaluate(()=>window.restoreClock());
 await sendLocation({x:11,y:12,updatedAt:Date.now()-60000});
 assert.equal(await page.getByRole('link',{name:'Join friend \u2197',exact:true}).count(),0);
 await sendLocation({x:11,y:12,updatedAt:Date.now()});
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor();
 await page.evaluate(peer=>{window.room.remoteParticipants.delete('peer');window.room.emit('left',{identity:peer});},peer);
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor({state:'hidden'});
 await page.evaluate(peer=>{window.room.remoteParticipants.set('peer',{identity:peer});window.room.emit('joined',{identity:peer});},peer);
 assert.equal(await page.getByRole('link',{name:'Join friend \u2197',exact:true}).count(),0);
 await sendLocation(null);
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor({state:'hidden'});
 explorerLocation={x:70,y:-12,updatedAt:Date.now()};
 // Location updates arrive through the automatic presence poll.
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor();
 assert.ok((await page.getByRole('link',{name:'Join friend \u2197',exact:true}).getAttribute('href')).includes('position=70,-12'));
 await page.evaluate(peer=>{window.room.remoteParticipants.delete('peer');window.room.emit('left',{identity:peer});},peer);
 assert.equal(await page.getByRole('link',{name:'Join friend \u2197',exact:true}).count(),1);
 explorerLocation=null;
 // Location updates arrive through the automatic presence poll.
 await page.getByRole('link',{name:'Join friend \u2197',exact:true}).waitFor({state:'hidden'});
 await page.waitForFunction(()=>!document.querySelector('.direct-composer textarea').disabled);
 console.log('PASS: Explorer presence supplies Join without a dcl.social broadcast or LiveKit participant; absent presence removes it.');
 const composer=page.getByRole('textbox',{name:'Direct message',exact:true});
 await composer.fill('Hello Ada'); await composer.press('Enter');
 await page.locator('.direct-message p').filter({hasText:'Hello Ada'}).waitFor();
 // A valid friend can be targeted before LiveKit announces their participant.
 await page.evaluate(peer=>{window.room.remoteParticipants.delete('peer');window.room.emit('left',{identity:peer});},peer);
 await composer.fill('Reply while participant discovery catches up');await composer.press('Enter');
 await page.locator('.direct-message p').filter({hasText:'Reply while participant discovery catches up'}).waitFor();
 const directTarget=await page.evaluate(()=>window.published.filter(p=>p.options.topic!=='dcl.social.location.v1').at(-1));
 assert.deepEqual(directTarget.options.destinationIdentities,[peer]);
 await page.evaluate(peer=>{window.room.remoteParticipants.set('peer',{identity:peer});window.room.emit('joined',{identity:peer});},peer);
 const sent=await page.evaluate(()=>window.published.filter(p=>p.options.topic!=='dcl.social.location.v1')[0]);
 assert.deepEqual(sent.options.destinationIdentities,[peer]);
 assert.equal(sent.options.reliable,true);
 assert.equal(sent.options.topic,peer); // Explorer rejects topic-less private messages.
 assert.equal(directTarget.options.topic,peer);
 assert.equal(Packet.decode(Uint8Array.from(sent.bytes)).message.chat.message,'Hello Ada');
 // Explorer constructs DateTime.FromOADate(chat.timestamp), not a Unix date.
 const timestamp=Packet.decode(Uint8Array.from(sent.bytes)).message.chat.timestamp;
 assert.ok(timestamp > -657435 && timestamp < 2958466,'Timestamp must be accepted by Explorer DateTime.FromOADate');
 assert.ok(Math.abs((timestamp-25569)*86400000-Date.now())<10000,'Explorer must display the current UTC date');
 const inbound=text=>Array.from(Packet.encode({protocolVersion:100,message:{$case:'chat',chat:{message:text,timestamp:Date.now()/86400000+25569}}}).finish());
 await page.evaluate(({bytes,peer,own})=>window.room.emit('data',Uint8Array.from(bytes),{identity:peer},undefined,own),{bytes:inbound('Explorer addressed reply'),peer,own});
 await page.getByText('Explorer addressed reply',{exact:true}).waitFor();
 for (const topic of ['community:example',other]) {
   await page.evaluate(({bytes,peer,topic})=>window.room.emit('data',Uint8Array.from(bytes),{identity:peer},undefined,topic),{bytes:inbound('Wrong conversation'),peer,topic});
 }
 assert.equal(await page.getByText('Wrong conversation',{exact:true}).count(),0);
 await page.evaluate(({bytes,peer})=>{window.room.remoteParticipants.delete('peer');window.room.emit('packet',{participantIdentity:peer,value:{case:'user',value:{payload:Uint8Array.from(bytes)}}});}, {bytes:inbound('First packet before participant discovery'),peer});
 await page.locator('.direct-message p').filter({hasText:'First packet before participant discovery'}).waitFor();
 await page.evaluate(({bytes,other})=>window.room.emit('packet',{participantIdentity:other,value:{case:'user',value:{payload:Uint8Array.from(bytes)}}}),{bytes:inbound('Untrusted first packet'),other});
 assert.equal(await page.getByText('Untrusted first packet',{exact:true}).count(),0);
 await page.evaluate(peer=>{window.room.remoteParticipants.set('peer',{identity:peer});window.room.emit('joined',{identity:peer});},peer);

 await page.evaluate(({bytes,peer})=>window.room.emit('data',Uint8Array.from(bytes),{identity:peer}),{bytes:inbound('https://decentraland.org/jump/?position=10,20'),peer});
 await page.locator('.direct-header').getByRole('link',{name:'Jump in \u2197',exact:true}).waitFor();
 await page.getByRole('button',{name:'Share a place with friend'}).click();
 await page.getByRole('textbox',{name:'Find a scene'}).fill('30,40');
 await page.getByRole('button',{name:/30, 40 Share this location/}).click();
 await page.getByRole('button',{name:'Share scene',exact:true}).click();
 await page.waitForFunction(()=>window.published.filter(p=>p.options.topic!=='dcl.social.location.v1').length===3);
 assert.equal(Packet.decode(Uint8Array.from((await page.evaluate(()=>window.published.filter(p=>p.options.topic!=='dcl.social.location.v1')[2])).bytes)).message.chat.message,'https://decentraland.org/jump/?position=30,40');
 await page.evaluate(({bytes,other})=>window.room.emit('data',Uint8Array.from(bytes),{identity:other}),{bytes:inbound('Unwanted message'),other});
 assert.equal(await page.getByText('Unwanted message',{exact:true}).count(),0);
 await page.screenshot({path:'/tmp/dcl-friends-desktop.png'});
 await page.setViewportSize({width:390,height:844});
 await page.getByRole('button',{name:'Back to friends'}).click();
 await page.getByRole('link',{name:/Ada Available to chat/}).click();
 await composer.waitFor();
 await page.screenshot({path:'/tmp/dcl-friends-mobile.png'});
 // A dropped RPC transport must recover without losing the draft or replaying sends.
 await composer.fill('Keep this draft during reconnect');
 const beforeReconnect=connections;
 for(const socket of wss.clients) socket.close(1012,'fixture restart');
 await page.locator('.friends-error:visible').filter({hasText:'Reconnecting\u2026'}).waitFor();
 await page.waitForFunction(()=>!document.querySelector('.direct-composer textarea').disabled);
 assert.equal(connections,beforeReconnect+1);
 assert.equal(await composer.inputValue(),'Keep this draft during reconnect');
 assert.equal(await page.evaluate(()=>window.published.filter(p=>p.options.topic!=='dcl.social.location.v1').length),0);
 assert.equal(await page.locator('.direct-message p').filter({hasText:'Hello Ada'}).count(),1);
 await composer.fill('');
 // A real protocol request keeps the browser WebSocket active every 30 seconds.
 await page.waitForTimeout(31000);
 assert.ok(heartbeats>0);
 console.log('PASS: automatic RPC recovery, subscriptions restored, draft/history retained, no message replay, periodic keepalive.');
 // Calling uses a second media room; keep the DM room for later sender checks.
 await page.evaluate(()=>window.dmRoom=window.room);
 const voiceEvent=(callId,status,caller=peer,callee=own)=>{for(const s of streams)if(s.voice)s.push({callId,status,caller:{address:caller},callee:{address:callee},credentials:{connectionUrl:'livekit:wss://voice.test.invalid?access_token=fixture'}});};
 await page.getByRole('button',{name:'Call',exact:true}).click();
 await page.getByRole('button',{name:'Cancel call',exact:true}).waitFor();
 await page.waitForTimeout(100);
 assert.deepEqual(calls[0],['start',peer]);
 voiceEvent('outgoing',1,own,peer);
 await page.getByRole('button',{name:'End call',exact:true}).waitFor();
 assert.equal(await page.evaluate(()=>window.room.localParticipant.isMicrophoneEnabled),false);
 await page.getByRole('button',{name:'Unmute call',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.room.localParticipant.isMicrophoneEnabled),true);
 await page.getByRole('button',{name:'Deafen call',exact:true}).click();
 await page.getByRole('button',{name:'Hear call',exact:true}).waitFor();
 assert.equal(await page.locator('#mobile-voice .call-card').count(),1);
 await page.screenshot({path:'/tmp/dcl-private-call-mobile.png'});
 await page.setViewportSize({width:1440,height:950});
 await page.locator('#friends-voice .call-card').waitFor();
 assert.equal(await page.locator('.call-card').evaluate(el=>getComputedStyle(el).position),'static');
 await page.screenshot({path:'/tmp/dcl-private-call-sidebar.png'});
 await page.evaluate(()=>location.hash='#/discover');
 await page.locator('#sidebar-voice .call-card').waitFor();
 await page.evaluate(peer=>location.hash='#/dm/'+peer,peer);
 await page.locator('#friends-voice .call-card').waitFor();
 await page.setViewportSize({width:390,height:844});
 await page.locator('#mobile-voice .call-card').waitFor();
 await page.getByRole('button',{name:'End call',exact:true}).click();
 await page.getByRole('complementary',{name:'Voice call'}).waitFor({state:'hidden'});
 voiceEvent('unknown',0,other);
 await page.waitForTimeout(100);
 assert.equal(await page.getByRole('button',{name:'Accept call',exact:true}).count(),0);
 voiceEvent('incoming-decline',0);
 await page.getByRole('button',{name:'Decline call',exact:true}).click();
 voiceEvent('incoming-accept',0);
 await page.getByRole('button',{name:'Accept call',exact:true}).click();
 await page.getByRole('button',{name:'End call',exact:true}).waitFor();
 voiceEvent('incoming-accept',3);
 await page.getByRole('complementary',{name:'Voice call'}).waitFor({state:'hidden'});
 assert.ok(calls.some(c=>c[0]==='reject'&&c[1]==='incoming-decline'));
 assert.ok(calls.some(c=>c[0]==='accept'&&c[1]==='incoming-accept'));
 assert.ok(calls.some(c=>c[0]==='end'&&c[1]==='outgoing'));
 await page.evaluate(()=>window.room=window.dmRoom);
 console.log('PASS: outgoing/incoming private calls, accept, decline, mute, deafen, remote end, unknown-caller rejection.');
 await page.setViewportSize({width:1440,height:950});
 await page.getByLabel('Friend options').click();
 await page.getByRole('button',{name:'Mutual friends',exact:true}).click();
 await page.getByRole('dialog',{name:'Mutual friends'}).getByRole('link',{name:'Ada'}).waitFor();
 await page.getByRole('button',{name:'Close dialog',exact:true}).click();
 await page.getByRole('button',{name:'Block',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('.direct-composer textarea').disabled);
 await page.evaluate(()=>location.hash='#/friends');
 await page.getByRole('button',{name:'Blocked',exact:true}).click();
 await page.getByRole('button',{name:'Unblock',exact:true}).click();
 await page.getByText('No blocked people.',{exact:true}).waitFor();
 await page.getByRole('button',{name:'Add friend',exact:true}).click();
 await page.getByLabel('Friend wallet address').fill(other);
 await page.getByRole('button',{name:'Send friend request',exact:true}).click();
 await page.getByRole('button',{name:'Request sent',exact:true}).waitFor();
 await page.getByRole('button',{name:'Close dialog',exact:true}).click();
 await page.getByRole('button',{name:'Requests',exact:true}).click();
 await page.getByRole('button',{name:'Cancel request',exact:true}).click();
 await page.getByText('No sent requests.',{exact:true}).waitFor();
 assert.ok(calls.some(c=>c[0]==='friendship'&&c[1]==='cancel'));
 await page.evaluate(peer=>location.hash='#/dm/'+peer,peer);
 const beforeHomeConnections=connections;
 const beforeHomeHistory=await page.locator('.direct-message').count();
 await page.evaluate(()=>location.hash='#/home');
 await page.locator('.home-friend').filter({hasText:'Ada'}).waitFor();
 assert.ok(await page.locator('.home-friend').filter({hasText:'Ada'}).locator('.is-online').count());
 await page.getByRole('button',{name:'Add a friend',exact:true}).click();
 await page.getByRole('dialog',{name:'Add a friend'}).waitFor();
 await page.getByRole('button',{name:'Close dialog',exact:true}).click();
 await page.locator('.home-friend').filter({hasText:'Ada'}).click();
 await page.getByRole('textbox',{name:'Direct message',exact:true}).waitFor();
 assert.equal(connections,beforeHomeConnections);
 assert.equal(await page.locator('.direct-message').count(),beforeHomeHistory);
 await page.evaluate(()=>location.hash='#/home');
 privacyFailure=true;
 await page.getByRole('button',{name:'Preferences',exact:true}).first().click();
 await page.getByRole('dialog',{name:'Preferences',exact:true}).getByRole('button',{name:'Privacy',exact:true}).click();
 await page.getByRole('button',{name:'Retry privacy settings',exact:true}).waitFor();privacyFailure=false;
 await page.getByRole('button',{name:'Retry privacy settings',exact:true}).click();
 await page.getByLabel('Who can send you private messages?').selectOption('0');
 await page.getByRole('button',{name:'Save privacy',exact:true}).click();
 await page.getByText('Saved',{exact:true}).waitFor();assert.equal(privacy.privateMessagesPrivacy,0);
 await page.getByRole('checkbox',{name:'Allow incoming friend calls'}).uncheck();
 voiceEvent('locally-disabled',0);await page.waitForTimeout(100);
 assert.equal(await page.getByRole('button',{name:'Accept call',exact:true}).count(),0);
 assert.ok(calls.some(c=>c[0]==='reject'&&c[1]==='locally-disabled'));
 await page.getByRole('checkbox',{name:'Allow incoming friend calls'}).check();
 await page.getByRole('button',{name:'Audio',exact:true}).click();
 await page.evaluate(()=>{const original=navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);navigator.mediaDevices.getUserMedia=async options=>{const stream=await original(options);window.testAudioTracks=stream.getTracks();return stream;};});
 await page.getByRole('button',{name:'Test microphone',exact:true}).click();
 await page.getByRole('button',{name:'Stop microphone test',exact:true}).waitFor();
 await page.getByRole('meter',{name:'Microphone input level'}).waitFor();
 assert.equal(await page.evaluate(()=>window.testAudioTracks[0].readyState),'live');
 await page.getByRole('button',{name:'Close dialog',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.testAudioTracks[0].readyState),'ended');
 await page.evaluate(peer=>location.hash='#/dm/'+peer,peer);
 console.log('PASS: Foundation privacy settings, browser-only call decline, real local mic test and capture cleanup.');
 console.log('PASS: Home friend state, add-friend modal, retained RPC connection and DM history.');
 console.log('PASS: mutual friends, block/unblock, add friend and cancel sent request.');
 blocked=true;for(const s of streams)if(s.block)s.push({address:peer,isBlocked:true});
 await page.waitForFunction(()=>document.querySelector('.direct-composer textarea').disabled);
 await page.evaluate(({bytes,peer})=>window.room.emit('data',Uint8Array.from(bytes),{identity:peer}),{bytes:inbound('Blocked message'),peer});
 assert.equal(await page.getByText('Blocked message',{exact:true}).count(),0);
 await page.setViewportSize({width:1440,height:950});
 await page.evaluate(community=>location.hash=`#/c/${community}/voice`,community);
 await page.getByRole('button',{name:'Join voice',exact:true}).click();
 await page.getByRole('button',{name:'Leave voice',exact:true}).waitFor();
 await page.evaluate(peer=>{window.remoteVoiceAudio=document.createElement('audio');window.room.emit('track',{kind:'audio',attach:()=>window.remoteVoiceAudio},{},{identity:peer});},peer);
 await page.getByRole('button',{name:'Mute for me',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.remoteVoiceAudio.muted),true);
 await page.getByRole('button',{name:'Deafen',exact:true}).click();
 await page.getByRole('button',{name:'Sound on',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.remoteVoiceAudio.muted),true);
 await page.getByRole('button',{name:'Unmute for me',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.remoteVoiceAudio.muted),false);
 await page.getByRole('button',{name:'Let speak',exact:true}).click();
 assert.ok(calls.some(c=>c[0]==='promote'&&c[1].communityId===community&&c[1].userAddress===peer));
 await page.getByRole('button',{name:'Decline',exact:true}).click();
 assert.ok(calls.some(c=>c[0]==='reject-hand'));
 await page.evaluate(()=>{const peer=window.room.remoteParticipants.get('peer');peer.permissions.canPublish=true;peer.metadata=JSON.stringify({role:'member',isSpeaker:true});window.room.emit('metadata');});
 await page.getByRole('button',{name:'Mute speaker',exact:true}).click();
 assert.ok(calls.some(c=>c[0]==='host-mute'&&c[1].muted));
 await page.evaluate(()=>{const peer=window.room.remoteParticipants.get('peer');peer.metadata=JSON.stringify({role:'member',isSpeaker:true,muted:true});window.room.emit('metadata');});
 await page.getByRole('button',{name:'Allow unmute',exact:true}).click();
 await page.getByRole('button',{name:'Move to listeners',exact:true}).click();
 await page.getByRole('button',{name:'Remove from voice',exact:true}).click();
 assert.ok(calls.some(c=>c[0]==='demote'));assert.ok(calls.some(c=>c[0]==='kick'));

 assert.equal(await page.evaluate(()=>window.room.localParticipant.isMicrophoneEnabled),false);
 await page.getByRole('button',{name:'Unmute',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.room.localParticipant.isMicrophoneEnabled),true);
 await page.evaluate(()=>{window.room.localParticipant.metadata=JSON.stringify({role:'owner',muted:true});window.room.emit('metadata');});
 await page.waitForFunction(()=>!window.room.localParticipant.isMicrophoneEnabled);
 await page.locator('.voice-stage').getByRole('button',{name:'Muted by host',exact:true}).waitFor();
 assert.equal(await page.locator('.voice-stage').getByRole('button',{name:'Muted by host',exact:true}).isDisabled(),true);
 await page.evaluate(()=>{window.room.localParticipant.metadata=JSON.stringify({role:'owner',muted:false});window.room.emit('metadata');});
 await page.getByRole('button',{name:'Unmute',exact:true}).click();
 await page.locator('.voice-stage').getByRole('button',{name:'Audio settings',exact:true}).click();
 await page.getByRole('button',{name:'Test microphone',exact:true}).click();
 await page.getByRole('button',{name:'Stop microphone test',exact:true}).waitFor();
 const microphone=await page.getByLabel('Microphone device').locator('option').evaluateAll(options=>options.find(o=>o.value!=='default')?.value);
 assert.ok(microphone);
 await page.evaluate(()=>window.failDeviceSwitch=true);
 await page.getByLabel('Microphone device').selectOption(microphone);
 await page.getByText('Test device unavailable',{exact:true}).waitFor();
 assert.equal(await page.getByLabel('Microphone device').inputValue(),'default');
 await page.evaluate(()=>window.failDeviceSwitch=false);
 await page.getByLabel('Microphone device').selectOption(microphone);
 await page.waitForFunction(id=>window.selectedAudio?.id===id,microphone);
 const originalDevices=await page.evaluate(()=>{window.originalEnumerate=navigator.mediaDevices.enumerateDevices.bind(navigator.mediaDevices);navigator.mediaDevices.enumerateDevices=async()=>[];navigator.mediaDevices.dispatchEvent(new Event('devicechange'));return true;});
 assert.ok(originalDevices);
 await page.waitForFunction(()=>window.selectedAudio?.id==='default');
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem('dcl.social.audio')).input==='default');
 await page.evaluate(()=>navigator.mediaDevices.enumerateDevices=window.originalEnumerate);
 await page.getByRole('button',{name:'Close dialog',exact:true}).click();
 await page.getByRole('button',{name:'Mute',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.room.localParticipant.isMicrophoneEnabled),false);
 await page.getByRole('button',{name:'Deafen',exact:true}).click();
 await page.getByRole('button',{name:'Sound on',exact:true}).waitFor();
 await page.screenshot({path:'/tmp/dcl-voice-desktop.png'});
 await page.getByRole('button',{name:'Leave voice',exact:true}).click();
 await page.getByRole('button',{name:'Join voice',exact:true}).waitFor();
 await page.getByRole('button',{name:'Join voice',exact:true}).click();
 await page.getByRole('button',{name:'End for everyone',exact:true}).waitFor();
 page.once('dialog',dialog=>dialog.accept());
 await page.getByRole('button',{name:'End for everyone',exact:true}).click();
 await page.getByRole('button',{name:'Join voice',exact:true}).waitFor();
 assert.ok(calls.some(c=>c[0]==='end-community'&&c[1].communityId===community));
 console.log('PASS: voice host hand approvals/rejections, promote/demote, mute/unmute, kick and end session.');
 console.log('PASS: Foundation voice signaling, muted join, unmute, mute, deafen and leave using fixture media transport.');
 assert.deepEqual(errors,[]);
 console.log('PASS: automatic friend-only location sharing, online-first ordering, live jump links, stale expiry, unknown-sender rejection; Foundation RPC friend list, targeted protobuf sends, incoming place cards, scene sharing, unknown/blocked senders, mobile navigation. LiveKit media transport is a fixture.');
 }finally {await browser?.close();for(const socket of wss?.clients || []) socket.terminate();web?.closeAllConnections();await new Promise(r=>web?web.close(r):r());await new Promise(r=>server ? server.httpServer.close(r) : r());rmSync(dir,{recursive:true,force:true});}
})().catch(e=>{console.error(e);process.exitCode=1});
