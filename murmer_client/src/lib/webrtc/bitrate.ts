/**
 * Cap every encoding of an outgoing sender at `maxBitrate` bits per second,
 * or lift the cap when it is `null` or not positive. Shared by voice, camera
 * and screen share senders.
 *
 * Errors are swallowed: a sender being torn down refuses new parameters, and
 * a cap that did not land only costs bandwidth until the next one does.
 */
export function capBitrate(
  sender: RTCRtpSender,
  maxBitrate: number | null,
  degradationPreference?: RTCDegradationPreference
): void {
  try {
    const params = sender.getParameters();
    if (!params.encodings || params.encodings.length === 0) params.encodings = [{}];
    for (const encoding of params.encodings) {
      if (maxBitrate && maxBitrate > 0) encoding.maxBitrate = maxBitrate;
      else delete encoding.maxBitrate;
    }
    if (degradationPreference) params.degradationPreference = degradationPreference;
    sender.setParameters(params).catch(() => {});
  } catch {
    // Ignore configuration errors
  }
}
