export type SetupProfile = "content-node" | "full-realm" | "public-gateway";
export type SetupTls = "acme-http01" | "acme-dns01" | "none";

export type SetupAnswers = {
  profile: SetupProfile;
  domain: string;
  tls: SetupTls;
  acmeEmail: string;
  adminAddresses: string;
  ethRpcUrl: string;
  squidEthRpc: string;
  squidPolygonRpc: string;
  sqdPortalKey: string;
  syncSources: string;
  livekitNodeIp: string;
  playEnabled: boolean;
  federationSeed: boolean;
};

export type SetupIssue = { field: keyof SetupAnswers; message: string };

export type SetupFile = { path: string; body: string };

export type SetupOutput = {
  hostNix: SetupFile;
  secrets: SetupFile[];
  checklist: string[];
};

export type SetupChange = <K extends keyof SetupAnswers>(
  field: K,
  value: SetupAnswers[K],
) => void;

export type ServerSetupPageProps = {
  answers: SetupAnswers;
  issues: SetupIssue[];
  output: SetupOutput;
  onChange: SetupChange;
  serverHref: string;
};
