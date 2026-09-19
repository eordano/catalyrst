const assert=require('node:assert/strict');
const path=require('node:path');
const {chromium}=require('playwright');
(async()=>{
 const {createServer}=await import('vite');
 const fixture=" import React,{useState} from 'react';import {createRoot} from 'react-dom/client';\n import {useActivity,ActivityInbox} from '/src/notifications.tsx';import {FriendsMap} from '/src/FriendsMap.tsx';\n const h=React.createElement;\n function Test(){const [wallet,setWallet]=useState('alice');const activity=useActivity(wallet);window.testNotify=activity.notify;window.testWallet=setWallet;\n const friend=(address,name,x,y,age=0,world)=>({address,name,available:true,location:{x,y,updatedAt:Date.now()-age,...(world?{world}:{})}});\n return h(React.Fragment,null,h(ActivityInbox,activity),h(FriendsMap,{friends:[friend('0x01','EY',10,20),friend('0x02','eordano',10,20),friend('0x03','Stale',50,60,60000),friend('0x04','World visitor',0,0,0,'party.dcl.eth')]}));}\n createRoot(document.getElementById('root')).render(h(Test));";
 const server=await createServer({plugins:[{name:'activity-fixture',resolveId(id){if(id==='/__activity-fixture.tsx')return id;},load(id){if(id==='/__activity-fixture.tsx')return fixture;}}],root:path.resolve(__dirname,'..'),server:{host:'127.0.0.1',port:0,hmr:false,watch:null}});
 server.middlewares.use('/__activity-test',(_req,res)=>{res.setHeader('Content-Type','text/html');res.end(`<div id="root"></div><script type="module" src="/__activity-fixture.tsx"></script>`);});
 server.middlewares.stack.unshift(server.middlewares.stack.pop());
 let browser;
 try{await server.listen();browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||chromium.executablePath(),args:['--no-sandbox']});const page=await browser.newPage();const errors=[];page.on('pageerror',e=>{errors.push(e.message);console.error(e.message)});await page.addInitScript(()=>{window.desktop=[];window.Notification=class{static permission='granted';constructor(title){window.desktop.push(title);}close(){}};localStorage.setItem('dcl.social.notifications','on');});await page.goto(server.resolvedUrls.local[0]+'__activity-test');await page.getByRole('heading',{name:'Notifications'}).waitFor();
 const notice={id:'event:one',title:'EY is going',body:'World party',href:'#/events/11111111-1111-4111-8111-111111111111',createdAt:Date.now(),kind:'event'};
 await page.evaluate(n=>{window.testNotify(n);window.testNotify(n)},notice);assert.equal(await page.getByText('EY is going',{exact:true}).count(),1);assert.equal(await page.evaluate(()=>window.desktop.length),1);
 await page.getByRole('button',{name:'Mark all read'}).click();assert.equal(await page.locator('.activity-inbox .unread').count(),0);
 await page.evaluate(()=>window.testWallet('bob'));await page.getByText('You\u2019re all caught up.').waitFor();await page.evaluate(()=>window.testWallet('alice'));await page.getByText('EY is going',{exact:true}).waitFor();
 assert.equal(await page.locator('.friends-map-marker').count(),1);await page.getByRole('button',{name:'EY, eordano at 10, 20'}).click();assert.equal(await page.locator('.friends-map-detail').getByRole('link',{name:'Join',exact:true}).count(),2);assert.match(await page.locator('.friends-map-detail').getByRole('link',{name:'Join',exact:true}).first().getAttribute('href'),/position=10/);
 for(const width of [320,390,1440]){await page.setViewportSize({width,height:900});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);}
 await page.getByRole('button',{name:'Clear',exact:true}).click();await page.getByText('You\u2019re all caught up.').waitFor();assert.deepEqual(errors,[]);console.log('PASS activity dedup/read/clear/wallet isolation, fresh grouped friend map and mobile widths.');
 }finally{await browser?.close();await server.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
