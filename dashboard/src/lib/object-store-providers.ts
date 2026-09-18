export interface ObjectStoreProviderPreset {
  /** Canonical value stored as `provider`; existing configs and the CLI use these. */
  value: string;
  label: string;
  /** Shown under the select when the provider is chosen. */
  hint: string;
  endpointPlaceholder: string;
  regionPlaceholder: string;
  /** Default region filled in when the provider is selected. */
  defaultRegion: string;
  /** Default addressing filled in when the provider is selected. */
  pathStyle: boolean;
  /** Endpoint shape a correct value must match; used for inline validation. */
  endpointPattern?: RegExp;
  endpointPatternMessage?: string;
}

/**
 * The daemon does not ship a provider list: `provider` is a label, and the only
 * behavior it drives is a region default and path-vs-virtual addressing. These
 * presets fill in those conventions, and every field stays editable so MinIO or
 * any unlisted S3-compatible service can be entered by hand.
 */
export const OBJECT_STORE_PROVIDERS: ObjectStoreProviderPreset[] = [
  {
    value: 'r2',
    label: 'Cloudflare R2',
    hint: 'Uses the account endpoint and path-style addressing.',
    endpointPlaceholder: 'https://<account>.r2.cloudflarestorage.com',
    regionPlaceholder: 'auto',
    defaultRegion: 'auto',
    pathStyle: true,
    endpointPattern: /^https:\/\/[a-z0-9-]+\.r2\.cloudflarestorage\.com\/?$/i,
    endpointPatternMessage: 'An R2 endpoint looks like https://<account>.r2.cloudflarestorage.com.',
  },
  {
    value: 'aws',
    label: 'Amazon S3',
    hint: 'Region is required and the bucket is addressed as a subdomain.',
    endpointPlaceholder: 'https://s3.us-east-1.amazonaws.com',
    regionPlaceholder: 'us-east-1',
    defaultRegion: 'us-east-1',
    pathStyle: false,
    endpointPattern: /^https:\/\/([a-z0-9-]+\.)?s3[.-][a-z0-9-]+\.amazonaws\.com\/?$/i,
    endpointPatternMessage:
      'An S3 endpoint looks like https://s3.us-east-1.amazonaws.com. You can also leave it blank to use the region default.',
  },
  {
    value: 'b2',
    label: 'Backblaze B2',
    hint: 'The region must match the bucket, for example us-west-004.',
    endpointPlaceholder: 'https://s3.us-west-004.backblazeb2.com',
    regionPlaceholder: 'us-west-004',
    defaultRegion: 'us-west-004',
    pathStyle: false,
    endpointPattern: /^https:\/\/s3\.[a-z0-9-]+\.backblazeb2\.com\/?$/i,
    endpointPatternMessage: 'A B2 endpoint looks like https://s3.<region>.backblazeb2.com.',
  },
  {
    value: 'minio',
    label: 'MinIO',
    hint: 'Self-hosted. Point the endpoint at your MinIO server.',
    endpointPlaceholder: 'https://minio.example.com',
    regionPlaceholder: 'us-east-1',
    defaultRegion: 'us-east-1',
    pathStyle: true,
  },
  {
    value: 'wasabi',
    label: 'Wasabi',
    hint: 'Region endpoints look like s3.<region>.wasabisys.com.',
    endpointPlaceholder: 'https://s3.us-east-1.wasabisys.com',
    regionPlaceholder: 'us-east-1',
    defaultRegion: 'us-east-1',
    pathStyle: true,
    endpointPattern: /^https:\/\/s3\.[a-z0-9-]+\.wasabisys\.com\/?$/i,
    endpointPatternMessage: 'A Wasabi endpoint looks like https://s3.<region>.wasabisys.com.',
  },
  {
    value: 'digitalocean',
    label: 'DigitalOcean Spaces',
    hint: 'Region endpoints look like <region>.digitaloceanspaces.com.',
    endpointPlaceholder: 'https://nyc3.digitaloceanspaces.com',
    regionPlaceholder: 'nyc3',
    defaultRegion: 'nyc3',
    pathStyle: false,
    endpointPattern: /^https:\/\/[a-z0-9-]+\.digitaloceanspaces\.com\/?$/i,
    endpointPatternMessage: 'A Spaces endpoint looks like https://<region>.digitaloceanspaces.com.',
  },
  {
    value: 'custom',
    label: 'Other S3-compatible service',
    hint: 'Enter the endpoint, region, and addressing your provider documents.',
    endpointPlaceholder: 'https://s3.example.com',
    regionPlaceholder: 'us-east-1',
    defaultRegion: 'us-east-1',
    pathStyle: true,
  },
];

export function findProviderPreset(value: string): ObjectStoreProviderPreset | undefined {
  return OBJECT_STORE_PROVIDERS.find((provider) => provider.value === value);
}
