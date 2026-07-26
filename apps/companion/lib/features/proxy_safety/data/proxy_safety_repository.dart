class ProxySafetyStatus {
  const ProxySafetyStatus({
    required this.isDeviceOwner,
    required this.isArmed,
    this.host,
    this.port,
    this.message,
  });

  final bool isDeviceOwner;
  final bool isArmed;
  final String? host;
  final int? port;
  final String? message;

  factory ProxySafetyStatus.fromMap(Map<Object?, Object?> values) =>
      ProxySafetyStatus(
        isDeviceOwner: values['isDeviceOwner'] == true,
        isArmed: values['isArmed'] == true,
        host: values['host'] as String?,
        port: values['port'] as int?,
        message: values['message'] as String?,
      );
}

abstract interface class ProxySafetyRepository {
  Future<ProxySafetyStatus> status();
  Future<ProxySafetyStatus> arm({required String host, required int port});
  Future<ProxySafetyStatus> disarm();
}
