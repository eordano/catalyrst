#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const src = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "src");

const GOV = ["governance/frames/GovernanceChrome", "GovernanceNavId"];
const MKT = ["marketplace/frames/MarketplaceChrome", "MarketplaceNavId"];
const EXP = ["explorer/frames/ExploreChrome", "TabId"];
const SIT = ["web/frames/SitesChrome", "SitesNavId"];

const JOBS = [
  ["governance/pages/GovernanceProposals.tsx", "tab", ...GOV],
  ["governance/pages/GvProjectsList.tsx", "tab", ...GOV],
  ["governance/pages/GvProjectDetail.tsx", "tab", ...GOV],
  ["governance/pages/GvSubmitPoll.tsx", "tab", ...GOV],
  ["governance/pages/GvTransparency.tsx", "tab", ...GOV],
  ["governance/pages/GvProjectUpdateDetail.tsx", "tab", ...GOV],
  ["governance/pages/GvProposalDetail.tsx", "tab", ...GOV],
  ["governance/pages/GvDebugAdmin.tsx", "tab", ...GOV],
  ["governance/pages/GvHomeLanding.tsx", "chromeTab", ...GOV],
  ["governance/workflows/GvBidVotingFlow.tsx", "active", ...GOV],
  ["marketplace/pages/MkAccountPage.tsx", "tab", ...MKT],
  ["marketplace/pages/MkAccountPage2.tsx", "tab", ...MKT],
  ["marketplace/pages/MkAccountCollectionsSection.tsx", "tab", ...MKT],
  ["marketplace/pages/MkAssetPage.tsx", "tab", ...MKT],
  ["marketplace/pages/MkBidPage.tsx", "tab", ...MKT],
  ["marketplace/pages/MkBidPage2.tsx", "tab", ...MKT],
  ["marketplace/pages/MkCancelSalePage.tsx", "tab", ...MKT],
  ["marketplace/pages/MkManageAssetPage.tsx", "tab", ...MKT],
  ["marketplace/pages/MkStoreSettingsEditor.tsx", "tab", ...MKT],
  ["marketplace/pages/MkCollectionPage.tsx", "navTab", ...MKT],
  ["marketplace/pages/MkMyBids.tsx", "tab", ...MKT],
  ["marketplace/pages/MkClaimNamePage.tsx", "active", ...MKT],
  ["marketplace/pages/MkOnSaleOnRentAccountSections.tsx", "navTab", ...MKT],
  ["explorer/pages/Communities.tsx", "tab", ...EXP],
  ["explorer/pages/Places.tsx", "tab", ...EXP],
  ["explorer/pages/Reel.tsx", "tab", ...EXP],
  ["explorer/pages/Settings.tsx", "tab", ...EXP],
  ["web/pages/StWhatSOnCreateEditHangout.tsx", "active", ...SIT],
];

let failures = 0;
for (const [rel, ident, chromePath, navType] of JOBS) {
  const file = join(src, rel);
  let text = readFileSync(file, "utf8");
  const stateRe = new RegExp(`(const \\[${ident}, set\\w+\\] = useState)(<string>)?(\\("[^"]*"\\))`);
  if (!stateRe.test(text)) {
    console.error(`no ${ident} state matched in ${rel}`);
    failures += 1;
    continue;
  }
  text = text.replace(stateRe, `$1<${navType}>$3`);
  if (!text.includes(`type ${navType}`)) {
    const chromeName = chromePath.split("/").pop();
    const defaultImport = new RegExp(`import ${chromeName}( from "[^"]*${chromeName}";)`);
    const namedImport = new RegExp(`import \\{ (.*) \\}( from "[^"]*${chromeName}";)`);
    if (defaultImport.test(text)) {
      text = text.replace(defaultImport, `import ${chromeName}, { type ${navType} }$1`);
    } else if (namedImport.test(text)) {
      text = text.replace(namedImport, `import { $1, type ${navType} }$2`);
    } else {
      console.error(`no ${chromeName} import found in ${rel}`);
      failures += 1;
      continue;
    }
  }
  writeFileSync(file, text);
  console.log(`typed ${ident} as ${navType} in ${rel}`);
}
if (failures) process.exit(1);
