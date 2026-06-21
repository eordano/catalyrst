using DCL.FeatureFlags;
using DCL.Multiplayer.Connections.DecentralandUrls;
using DCL.Utility;
using ECS;
using System;
using System.Collections.Generic;

namespace DCL.Browser
{
    /// <summary>
    ///     Points the explorer at a catalyrst deployment.
    ///
    ///     Every Decentraland backend lives behind a single front host
    ///     (<see cref="CATALYRST_BASE" />, e.g. https://catalyst.dcl.one) that
    ///     nginx path-routes to the bundle that owns the upstream service.
    ///     The upstream subdomain (places, comms-gatekeeper, social-api, ...)
    ///     becomes a path prefix; nginx strips it and forwards the remaining
    ///     path verbatim to the bundle, where the member crate serves its real
    ///     routes (/api/places, /get-scene-adapter, /v1/communities, ...).
    ///
    ///     wss:// services keep their scheme; only the host is rewritten.
    ///     External links (x.com, discord, coingecko, opensea, decentraland.org
    ///     web app, docs, blog) are left untouched.
    ///
    ///     Realm-discovered URLs (Lambdas, Content, EntitiesDeployment, and the
    ///     realm-served EntitiesActive/WorldEntitiesActive fallbacks) are not
    ///     rewritten here: they come from the realm /about document. Set the
    ///     /about content.publicUrl and lambdas.publicUrl to the catalyrst
    ///     content-server (port 5141, fronted at {CATALYRST_BASE}/content and
    ///     {CATALYRST_BASE}/lambdas) so they resolve to us. See
    ///     docs/explorer-pointing/README.md.
    /// </summary>
    public class CatalyrstUrlsSource : DecentralandUrlsSource
    {
        private const string DEFAULT_BASE = "https://catalyst.dcl.one";

        private static readonly string[] DECENTRALAND_DOMAINS =
        {
            ".decentraland.org",
            ".decentraland.zone",
            ".decentraland.today",
        };

        private readonly string httpsBase;
        private readonly string wssBase;
        private readonly Dictionary<string, string> hostPrefix;

        public CatalyrstUrlsSource(
            DecentralandEnvironment environment,
            IRealmData realmData,
            ILaunchMode launchMode,
            string? catalyrstBase = null,
            string? gatekeeperBaseOverride = null)
            : base(environment, realmData, launchMode, gatekeeperBaseOverride)
        {
            string baseUrl = (catalyrstBase ?? ResolveBaseFromEnv()).TrimEnd('/');
            httpsBase = baseUrl;

            wssBase = baseUrl.StartsWith("https://", StringComparison.OrdinalIgnoreCase)
                ? "wss://" + baseUrl.Substring("https://".Length)
                : baseUrl.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
                    ? "ws://" + baseUrl.Substring("http://".Length)
                    : baseUrl;

            hostPrefix = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                { "places", "/places" },
                { "events", "/events" },
                { "archipelago-ea-stats", "/archipelago" },
                { "realm-provider-ea", "/realm-provider" },
                { "worlds-content-server", "/worlds-content-server" },
                { "api", "/map-api" },
                { "dcl-lists", "/lists" },

                { "builder-api", "/builder-api" },
                { "camera-reel-service", "/camera-reel" },
                { "asset-bundle-registry", "/ab-registry" },

                { "social-api", "/social-api" },
                { "comms-gatekeeper", "/comms-gatekeeper" },
                { "notifications", "/notifications" },
                { "badges", "/badges" },
                { "metamorph-api", "/media" },
                { "autotranslate-server", "/media" },
                { "assets-cdn", "/assets-cdn" },

                { "market", "/market" },
                { "credits", "/credits" },
                { "rpc-social-service-ea", "/social-rpc" },
                { "rpc", "/rpc" },

                { "ab-cdn", "/ab-cdn" },

                { "auth-api", "/auth-api" },
                { "transactions-api", "/transactions-api" },
                { "feature-flags", "/feature-flags" },
                { "config", "/config" },
                { "peer", "/peer" },
            };
        }

        public new static CatalyrstUrlsSource CreateForTest(DecentralandEnvironment environment, ILaunchMode launchMode) =>
            new (environment, new IRealmData.Fake(), launchMode);

        private static string ResolveBaseFromEnv()
        {
            string? fromEnv = Environment.GetEnvironmentVariable("CATALYRST_BASE");
            return string.IsNullOrEmpty(fromEnv) ? DEFAULT_BASE : fromEnv!;
        }

        protected override UrlData RawUrl(DecentralandUrl decentralandUrl)
        {
            UrlData serviceUrl = base.RawUrl(decentralandUrl);

            if (serviceUrl.Url == null)
                return serviceUrl;

            string? rewritten = Rewrite(serviceUrl.Url);

            if (rewritten == null)
                return serviceUrl;

            return new UrlData(serviceUrl.Caching, rewritten);
        }

        /// <summary>
        ///     Rewrites a 3rd party URL the same way <see cref="RawUrl" /> does,
        ///     so resources discovered at runtime (scene-supplied, profile
        ///     images, NFT image hosts that live on a decentraland subdomain)
        ///     also land on the catalyrst front host.
        /// </summary>
        public override string TransformUrl(string originalUrl) =>
            Rewrite(originalUrl) ?? originalUrl;

        public override string GetOriginalUrl(string url) =>
            url;

        private string? Rewrite(string url)
        {
            int schemeLen = SchemeLength(url, out bool secureWs);

            if (schemeLen == 0)
                return null;

            ReadOnlySpan<char> afterScheme = url.AsSpan(schemeLen);

            int slash = afterScheme.IndexOf('/');
            ReadOnlySpan<char> host = slash >= 0 ? afterScheme.Slice(0, slash) : afterScheme;
            string path = slash >= 0 ? afterScheme.Slice(slash).ToString() : string.Empty;

            string hostStr = host.ToString();
            string? subdomain = MatchDecentralandSubdomain(hostStr);

            if (subdomain == null)
                return null;

            if (!hostPrefix.TryGetValue(subdomain, out string? prefix))
                return null;

            string root = secureWs ? wssBase : httpsBase;
            return root + prefix + path;
        }

        private static int SchemeLength(string url, out bool isWs)
        {
            isWs = false;

            if (url.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                return "https://".Length;

            if (url.StartsWith("wss://", StringComparison.OrdinalIgnoreCase))
            {
                isWs = true;
                return "wss://".Length;
            }

            return 0;
        }

        private static string? MatchDecentralandSubdomain(string host)
        {
            foreach (string domain in DECENTRALAND_DOMAINS)
            {
                if (!host.EndsWith(domain, StringComparison.OrdinalIgnoreCase))
                    continue;

                int subLen = host.Length - domain.Length;

                if (subLen <= 0)
                    return null;

                return host.Substring(0, subLen);
            }

            return null;
        }
    }
}
