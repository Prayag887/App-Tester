class ProxySafetyStatus {
  const ProxySafetyStatus({
    required this.isMonitoring,
    this.host,
    this.port,
    this.message,
    this.logs = const [],
  });

  final bool isMonitoring;
  final String? host;
  final int? port;
  final String? message;
  final List<CompanionLogEntry> logs;

  factory ProxySafetyStatus.fromMap(Map<Object?, Object?> values) =>
      ProxySafetyStatus(
        isMonitoring: values['isMonitoring'] == true,
        host: values['host'] as String?,
        port: values['port'] as int?,
        message: values['message'] as String?,
        logs: ((values['logs'] as List<Object?>?) ?? const [])
            .whereType<Map<Object?, Object?>>()
            .map(CompanionLogEntry.fromMap)
            .toList(),
      );
}

class CompanionLogEntry {
  const CompanionLogEntry({required this.time, required this.message});
  final String time;
  final String message;
  factory CompanionLogEntry.fromMap(Map<Object?, Object?> values) =>
      CompanionLogEntry(time: values['time'] as String? ?? '', message: values['message'] as String? ?? '');
}

abstract interface class ProxySafetyRepository {
  Future<ProxySafetyStatus> status();
  Future<ProxySafetyStatus> startMonitoring({required String host, required int port});
  Future<ProxySafetyStatus> stopMonitoring();
}
